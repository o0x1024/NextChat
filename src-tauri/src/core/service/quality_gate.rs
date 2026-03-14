use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::domain::SystemSettings;
use crate::core::llm_rig::simple_complete;
use crate::core::workflow::{StageStatus, WorkflowStatus};

use super::AppService;

/// Outcome of a quality gate evaluation for a stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGateOutcome {
    /// Stage deliverables meet the goal — proceed to next stage.
    Pass,
    /// Stage deliverables do not meet the goal — workflow enters NeedsReview.
    Fail,
    /// Gate could not be evaluated (no LLM, parsing error, etc.) — treat as Pass.
    Skipped,
}

/// Result of a quality gate check, stored as JSON in `workflow_stages.quality_gate_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGateResult {
    pub outcome: QualityGateOutcome,
    /// 0.0–1.0 confidence in the outcome.
    pub confidence: f64,
    /// Short LLM-provided explanation.
    pub reasoning: String,
    /// Issues the LLM identified (empty on Pass).
    pub issues: Vec<String>,
}

const QUALITY_GATE_SYSTEM_PROMPT: &str = r#"You are a software quality reviewer in a multi-agent system.

A workflow stage has completed. Your job is to verify whether the stage deliverables satisfy the stage goal.

Respond with ONLY a JSON object (no markdown, no code fences):
{
  "outcome": "pass" | "fail",
  "confidence": 0.0-1.0,
  "reasoning": "brief explanation (1-2 sentences)",
  "issues": ["issue1", "issue2"]
}

A "pass" means the deliverables substantially achieve the goal. Minor imperfections are acceptable.
A "fail" means the deliverables miss the goal significantly or key criteria are unmet.
"issues" should be empty for a pass.
"#;

fn build_quality_gate_prompt(stage_title: &str, stage_goal: &str, deliverables_text: &str) -> String {
    format!(
        r#"Stage: {}
Goal: {}

Deliverables:
{}

Does this satisfy the goal? Reply with the JSON object."#,
        stage_title, stage_goal, deliverables_text
    )
}

fn parse_quality_gate_response(raw: &str) -> QualityGateResult {
    let cleaned = raw.trim();
    let stripped = if let Some(s) = cleaned.strip_prefix("```json") {
        s.trim()
    } else if let Some(s) = cleaned.strip_prefix("```") {
        s.trim()
    } else {
        cleaned
    };
    let stripped = stripped.strip_suffix("```").map(|s| s.trim()).unwrap_or(stripped);

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stripped) {
        let outcome_str = v.get("outcome").and_then(|o| o.as_str()).unwrap_or("pass");
        let outcome = if outcome_str == "fail" {
            QualityGateOutcome::Fail
        } else {
            QualityGateOutcome::Pass
        };
        let confidence = v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.8);
        let reasoning = v
            .get("reasoning")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();
        let issues = v
            .get("issues")
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        QualityGateResult { outcome, confidence, reasoning, issues }
    } else {
        // Unparseable — default to Skipped to avoid false negatives
        QualityGateResult {
            outcome: QualityGateOutcome::Skipped,
            confidence: 0.0,
            reasoning: format!("Failed to parse gate response: {}", &raw[..raw.len().min(200)]),
            issues: vec![],
        }
    }
}

impl AppService {
    /// Run the quality gate for a completed stage.
    ///
    /// If the gate fails, this updates the stage status to `NeedsReview` and the workflow
    /// status to `NeedsReview`, then returns the result so the caller can emit a notification.
    ///
    /// Returns `None` if quality gate evaluation was skipped (no LLM, no deliverables, no runtime).
    pub fn run_quality_gate(
        &self,
        workflow_id: &str,
        stage_id: &str,
        stage_title: &str,
        stage_goal: &str,
        deliverables_json: Option<&str>,
        settings: &SystemSettings,
    ) -> Result<Option<QualityGateResult>> {
        // If no deliverables or no goal, skip
        let deliverables_text = match deliverables_json {
            Some(json_str) if !json_str.is_empty() => {
                // Parse the deliverables array into readable text
                if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                    arr.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let summary = item
                                .get("summary")
                                .or_else(|| item.get("task_summary"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("(no summary)");
                            format!("{}. {}", i + 1, summary)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    json_str.to_string()
                }
            }
            _ => return Ok(None), // No deliverables — skip gate
        };

        if stage_goal.trim().is_empty() {
            return Ok(None);
        }

        // Resolve LLM config
        let config = match settings
            .providers
            .iter()
            .find(|p| p.id == settings.global_config.default_llm_provider)
        {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        if config.api_key.is_empty() {
            return Ok(None);
        }

        // Gate evaluation requires an async runtime
        let handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(_) => return Ok(None),
        };

        let user_prompt = build_quality_gate_prompt(stage_title, stage_goal, &deliverables_text);
        let model = settings.global_config.default_llm_model.clone();
        let proxy_url = settings.global_config.proxy_url.clone();

        let raw = tokio::task::block_in_place(|| {
            handle.block_on(simple_complete(
                &config,
                &proxy_url,
                &model,
                QUALITY_GATE_SYSTEM_PROMPT,
                &user_prompt,
                512,
            ))
        });

        let raw = match raw {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };

        let result = parse_quality_gate_response(&raw);

        // Persist gate result in stage
        let gate_json = serde_json::to_string(&result).unwrap_or_default();
        let mut stage = self.storage.get_workflow_stage(stage_id)?;
        stage.quality_gate_json = Some(gate_json);

        if result.outcome == QualityGateOutcome::Fail {
            stage.status = StageStatus::NeedsReview;
            self.storage.insert_workflow_stage(&stage)?;

            // Also mark workflow as NeedsReview
            let mut workflow = self.storage.get_workflow(workflow_id)?;
            workflow.status = WorkflowStatus::NeedsReview;
            self.storage.insert_workflow(&workflow)?;
        } else {
            self.storage.insert_workflow_stage(&stage)?;
        }

        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pass_response() {
        let raw = r#"{"outcome":"pass","confidence":0.9,"reasoning":"All goals met","issues":[]}"#;
        let result = parse_quality_gate_response(raw);
        assert_eq!(result.outcome, QualityGateOutcome::Pass);
        assert!((result.confidence - 0.9).abs() < 0.01);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn parses_fail_response() {
        let raw = r#"{"outcome":"fail","confidence":0.8,"reasoning":"Missing tests","issues":["No unit tests","Coverage below threshold"]}"#;
        let result = parse_quality_gate_response(raw);
        assert_eq!(result.outcome, QualityGateOutcome::Fail);
        assert_eq!(result.issues.len(), 2);
    }

    #[test]
    fn handles_code_fence() {
        let raw = "```json\n{\"outcome\":\"pass\",\"confidence\":0.7,\"reasoning\":\"OK\",\"issues\":[]}\n```";
        let result = parse_quality_gate_response(raw);
        assert_eq!(result.outcome, QualityGateOutcome::Pass);
    }

    #[test]
    fn handles_invalid_json() {
        let result = parse_quality_gate_response("not json at all");
        assert_eq!(result.outcome, QualityGateOutcome::Skipped);
    }
}

