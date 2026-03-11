use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const ASK_USER_QUESTION_PREFIX: &str = "__nextchat_ask_user_question__:";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestionToolInput {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub context: Option<String>,
    pub allow_free_form: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestionSignal {
    pub question: String,
    pub options: Vec<String>,
    pub context: Option<String>,
    pub allow_free_form: bool,
}

impl AskUserQuestionSignal {
    pub fn from_input(input: AskUserQuestionToolInput) -> Result<Self> {
        let question = input.question.trim().to_string();
        if question.is_empty() {
            bail!("AskUserQuestion question cannot be empty");
        }
        let mut options = input
            .options
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        options.dedup();
        if options.len() > 6 {
            bail!("AskUserQuestion supports at most 6 options");
        }
        let context = input
            .context
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(Self {
            question,
            options,
            context,
            allow_free_form: input.allow_free_form.unwrap_or(true),
        })
    }

    pub fn to_error_message(&self) -> Result<String> {
        let payload =
            serde_json::to_string(self).context("failed to serialize AskUserQuestion payload")?;
        Ok(format!("{ASK_USER_QUESTION_PREFIX}{payload}"))
    }
}

pub fn parse_signal_from_error(message: &str) -> Result<Option<AskUserQuestionSignal>> {
    let payload = if let Some(payload) = message.strip_prefix(ASK_USER_QUESTION_PREFIX) {
        payload
    } else if let Some(index) = message.find(ASK_USER_QUESTION_PREFIX) {
        &message[index + ASK_USER_QUESTION_PREFIX.len()..]
    } else {
        return Ok(None);
    };
    let signal = serde_json::from_str::<AskUserQuestionSignal>(payload)
        .map_err(|error| anyhow!("failed to parse AskUserQuestion payload: {error}"))?;
    Ok(Some(signal))
}

#[cfg(test)]
mod tests {
    use super::{parse_signal_from_error, AskUserQuestionSignal};

    #[test]
    fn parse_signal_accepts_wrapped_tool_errors() {
        let signal = AskUserQuestionSignal {
            question: "继续吗？".into(),
            options: vec!["是".into(), "否".into()],
            context: None,
            allow_free_form: false,
        };
        let message = format!(
            "Toolset error: ToolCallError: {}",
            signal.to_error_message().expect("serialize signal")
        );

        let parsed = parse_signal_from_error(&message)
            .expect("parse signal")
            .expect("signal present");

        assert_eq!(parsed.question, signal.question);
        assert_eq!(parsed.options, signal.options);
        assert_eq!(parsed.allow_free_form, signal.allow_free_form);
    }
}
