use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use serde_json::json;

use super::{
    block_on_service_future,
    owner_orchestration::{
        clean_optional_text, extract_candidates, extract_prompt_value, require_text, truncate_text,
    },
    AppService,
};
use crate::core::domain::{AgentProfile, TaskCard, WorkGroup};
use crate::core::workflow::{
    OwnerBlockerDecision, OwnerBlockerResolution, TaskBlockerRecord, TaskDispatchRecord,
};

impl AppService {
    pub(super) fn build_owner_blocker_decision(
        &self,
        work_group: &WorkGroup,
        task: &TaskCard,
        blocker: &TaskBlockerRecord,
        dispatch: Option<&TaskDispatchRecord>,
        members: &[AgentProfile],
    ) -> Result<OwnerBlockerDecision> {
        let owner = self
            .group_owner_for_work_group(&work_group.id)?
            .context("work group owner missing")?;
        let member_pool = members
            .iter()
            .filter(|agent| agent.id != owner.id)
            .cloned()
            .collect::<Vec<_>>();
        if member_pool.is_empty() {
            return Err(anyhow!(
                "owner orchestrated blocker handling requires at least one non-owner agent"
            ));
        }

        let decision = block_on_service_future(self.complete_owner_json::<OwnerBlockerDecision>(
            &owner,
            "owner.blocker_resolution",
            build_owner_blocker_prompt(work_group, task, blocker, dispatch, &member_pool),
            json!({
                "workGroupId": work_group.id,
                "taskId": task.id,
                "blockerId": blocker.id,
                "actorId": owner.id,
            }),
        ))?;
        self.validate_owner_blocker_decision(task, dispatch, &member_pool, decision)
    }

    fn validate_owner_blocker_decision(
        &self,
        task: &TaskCard,
        dispatch: Option<&TaskDispatchRecord>,
        members: &[AgentProfile],
        decision: OwnerBlockerDecision,
    ) -> Result<OwnerBlockerDecision> {
        let owner_narrative_text = require_text(
            decision.owner_narrative_text,
            "owner blocker narrative text missing",
        )?;
        let valid_assignees = members
            .iter()
            .map(|agent| agent.id.clone())
            .collect::<HashSet<_>>();
        let resolution = match decision.resolution {
            OwnerBlockerResolution::ProvideContext { message } => {
                OwnerBlockerResolution::ProvideContext {
                    message: require_text(Some(message), "owner blocker context missing")?,
                }
            }
            OwnerBlockerResolution::ReassignTask {
                target_agent_id,
                message,
            } => {
                let target_agent_id =
                    require_text(Some(target_agent_id), "owner blocker target agent missing")?;
                if !valid_assignees.contains(&target_agent_id) {
                    return Err(anyhow!("owner blocker chose an unknown target agent"));
                }
                if dispatch.is_some_and(|item| item.locked_by_user_mention) {
                    return Err(anyhow!(
                        "owner blocker cannot reassign a task locked by a user mention"
                    ));
                }
                OwnerBlockerResolution::ReassignTask {
                    target_agent_id,
                    message: require_text(Some(message), "owner blocker reassign message missing")?,
                }
            }
            OwnerBlockerResolution::CreateDependencyTask {
                target_agent_id,
                title,
                goal,
                message,
            } => {
                let target_agent_id = require_text(
                    Some(target_agent_id),
                    "owner blocker dependency agent missing",
                )?;
                if !valid_assignees.contains(&target_agent_id) {
                    return Err(anyhow!("owner blocker chose an unknown dependency agent"));
                }
                OwnerBlockerResolution::CreateDependencyTask {
                    target_agent_id,
                    title: require_text(Some(title), "owner blocker dependency title missing")?
                        .chars()
                        .take(72)
                        .collect(),
                    goal: require_text(Some(goal), "owner blocker dependency goal missing")?,
                    message: require_text(
                        Some(message),
                        "owner blocker dependency narrative missing",
                    )?,
                }
            }
            OwnerBlockerResolution::RequestApproval {
                question,
                options,
                context,
                allow_free_form,
            } => OwnerBlockerResolution::RequestApproval {
                question: require_text(Some(question), "owner blocker approval question missing")?,
                options: options
                    .into_iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .take(4)
                    .collect(),
                context: clean_optional_text(context),
                allow_free_form,
            },
            OwnerBlockerResolution::AskUser {
                question,
                options,
                context,
                allow_free_form,
            } => OwnerBlockerResolution::AskUser {
                question: require_text(Some(question), "owner blocker user question missing")?,
                options: options
                    .into_iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .take(4)
                    .collect(),
                context: clean_optional_text(context),
                allow_free_form,
            },
            OwnerBlockerResolution::PauseTask { message } => OwnerBlockerResolution::PauseTask {
                message: require_text(Some(message), "owner blocker pause message missing")?,
            },
        };

        if matches!(
            resolution,
            OwnerBlockerResolution::ProvideContext { .. }
                | OwnerBlockerResolution::ReassignTask { .. }
        ) && task.assigned_agent_id.is_none()
        {
            return Err(anyhow!(
                "owner blocker resolution requires an assigned task"
            ));
        }

        Ok(OwnerBlockerDecision {
            owner_narrative_text: Some(owner_narrative_text),
            resolution,
        })
    }
}

fn build_owner_blocker_prompt(
    work_group: &WorkGroup,
    task: &TaskCard,
    blocker: &TaskBlockerRecord,
    dispatch: Option<&TaskDispatchRecord>,
    members: &[AgentProfile],
) -> String {
    let member_lines = members
        .iter()
        .map(|agent| {
            format!(
                "- id: {}\n  name: {}\n  role: {}\n  objective: {}",
                agent.id,
                agent.name,
                agent.role,
                truncate_text(&agent.objective, 160),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "你需要处理一个由群成员上报给群主的阻塞，并返回唯一一个最合适的处理决策。\n\
只返回 JSON，格式必须是：\n\
{{\"ownerNarrativeText\":\"群主在主聊天里的发言\",\"resolution\":{{\"action\":\"provide_context|reassign_task|create_dependency_task|request_approval|ask_user|pause_task\",...}}}}\n\
规则：\n\
- ownerNarrativeText 必填，用自然中文表达群主决定。\n\
- provide_context: 字段 message。\n\
- reassign_task: 字段 targetAgentId, message。\n\
- create_dependency_task: 字段 targetAgentId, title, goal, message。\n\
- request_approval: 字段 question, options, context, allowFreeForm。\n\
- ask_user: 字段 question, options, context, allowFreeForm。\n\
- pause_task: 字段 message。\n\
- 只能选择给定候选成员，不能指派群主自己。\n\
- 如果任务被用户显式点名锁定，不要返回 reassign_task。\n\
- ownerNarrativeText 必须和 resolution 保持一致。\n\
工作组：{} / {}\n\
任务标题：{}\n\
任务目标：{}\n\
当前执行人 agentId：{}\n\
workflowId：{}\n\
stageId：{}\n\
stageTitle：{}\n\
blockerCategory：{:?}\n\
blockerSummary：{}\n\
blockerDetails：{}\n\
任务是否被用户点名锁定：{}\n\
候选成员：\n{}\n",
        work_group.name,
        work_group.goal,
        task.title,
        task.normalized_goal,
        task.assigned_agent_id.clone().unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.workflow_id.clone())
            .unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.stage_id.clone())
            .unwrap_or_else(|| "none".into()),
        dispatch
            .and_then(|item| item.narrative_stage_label.clone())
            .unwrap_or_else(|| "none".into()),
        blocker.category,
        blocker.summary,
        blocker.details,
        dispatch.is_some_and(|item| item.locked_by_user_mention),
        member_lines,
    )
}

pub(super) fn mock_owner_blocker_completion(prompt: &str) -> String {
    let category = extract_prompt_value(prompt, "blockerCategory：").unwrap_or_default();
    let summary = extract_prompt_value(prompt, "blockerSummary：").unwrap_or_default();
    let current_assignee = extract_prompt_value(prompt, "当前执行人 agentId：").unwrap_or_default();
    let user_locked = extract_prompt_value(prompt, "任务是否被用户点名锁定：")
        .is_some_and(|value| value == "true");
    let candidates = extract_candidates(prompt);
    let fallback_target = candidates
        .iter()
        .find(|candidate| candidate.id != current_assignee)
        .or_else(|| candidates.first())
        .cloned()
        .unwrap_or_default();

    if category.contains("NeedUserDecision") || summary.contains("风格") {
        json!({
            "ownerNarrativeText": "这个阻塞我先接住，先把关键选择问清楚，确认后马上继续推进。",
            "resolution": {
                "action": "ask_user",
                "question": "登录页要采用管理后台风格还是简洁读者端风格？",
                "options": ["管理后台风格", "简洁读者端风格"],
                "context": "当前缺少页面结构与视觉方向。",
                "allowFreeForm": false
            }
        })
        .to_string()
    } else if category.contains("PermissionRequired") || summary.contains("审批") {
        json!({
            "ownerNarrativeText": "这个阻塞需要审批，我先向用户发起审批确认，确认后再继续执行。",
            "resolution": {
                "action": "request_approval",
                "question": "是否批准当前生产发布时间窗？",
                "options": ["批准", "拒绝"],
                "context": "发布前需要最终审批。",
                "allowFreeForm": false
            }
        })
        .to_string()
    } else if summary.contains("架构") || summary.contains("依赖") {
        json!({
            "ownerNarrativeText": format!("这个阻塞需要先补前置依赖，我让 {} 先把依赖项补齐。", fallback_target.name),
            "resolution": {
                "action": "create_dependency_task",
                "targetAgentId": fallback_target.id,
                "title": "补借阅状态与接口约束",
                "goal": "补充借阅状态流转和接口约束",
                "message": "请先补借阅状态流转和接口约束。"
            }
        })
        .to_string()
    } else if !user_locked && candidates.len() > 1 && summary.contains("更合适") {
        json!({
            "ownerNarrativeText": format!("这个问题我改派 {} 来接更合适，你先继续往下推进。", fallback_target.name),
            "resolution": {
                "action": "reassign_task",
                "targetAgentId": fallback_target.id,
                "message": "你先接手当前阻塞，确认后把处理结论同步回来。"
            }
        })
        .to_string()
    } else {
        json!({
            "ownerNarrativeText": "这个阻塞我来补充关键上下文，原执行人按更新后的约束继续推进。",
            "resolution": {
                "action": "provide_context",
                "message": "缺失的约束和前置说明我已经补齐，按更新后的上下文继续推进。"
            }
        })
        .to_string()
    }
}
