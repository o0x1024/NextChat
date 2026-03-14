use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const REQUEST_PEER_INPUT_PREFIX: &str = "__nextchat_request_peer_input__:";

/// Input that an agent provides when calling the RequestPeerInput tool.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPeerInputToolInput {
    /// The ID of the peer agent to request input from.
    pub target_agent_id: String,
    /// The question or sub-task description for the peer agent.
    pub question: String,
    /// Optional additional context to share with the peer agent.
    pub context: Option<String>,
}

/// Signal emitted as an error string to unwind the LLM tool-call loop and
/// hand control back to the service layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPeerInputSignal {
    pub target_agent_id: String,
    pub question: String,
    pub context: Option<String>,
}

impl RequestPeerInputSignal {
    pub fn from_input(input: RequestPeerInputToolInput) -> Result<Self> {
        let target_agent_id = input.target_agent_id.trim().to_string();
        if target_agent_id.is_empty() {
            bail!("RequestPeerInput: targetAgentId cannot be empty");
        }
        let question = input.question.trim().to_string();
        if question.is_empty() {
            bail!("RequestPeerInput: question cannot be empty");
        }
        let context = input
            .context
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty());
        Ok(Self {
            target_agent_id,
            question,
            context,
        })
    }

    pub fn to_error_message(&self) -> Result<String> {
        let payload =
            serde_json::to_string(self).context("failed to serialize RequestPeerInput payload")?;
        Ok(format!("{REQUEST_PEER_INPUT_PREFIX}{payload}"))
    }
}

pub fn parse_peer_input_signal(message: &str) -> Result<Option<RequestPeerInputSignal>> {
    let payload = if let Some(payload) = message.strip_prefix(REQUEST_PEER_INPUT_PREFIX) {
        payload
    } else if let Some(index) = message.find(REQUEST_PEER_INPUT_PREFIX) {
        &message[index + REQUEST_PEER_INPUT_PREFIX.len()..]
    } else {
        return Ok(None);
    };
    let signal = serde_json::from_str::<RequestPeerInputSignal>(payload)
        .map_err(|error| anyhow!("failed to parse RequestPeerInput payload: {error}"))?;
    Ok(Some(signal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_signal_from_error_message() {
        let signal = RequestPeerInputSignal {
            target_agent_id: "agent-42".into(),
            question: "请分析依赖冲突".into(),
            context: Some("当前版本是 2.1".into()),
        };
        let error_msg = signal.to_error_message().unwrap();
        let parsed = parse_peer_input_signal(&error_msg)
            .unwrap()
            .expect("should parse signal");
        assert_eq!(parsed.target_agent_id, "agent-42");
        assert_eq!(parsed.question, "请分析依赖冲突");
        assert_eq!(parsed.context.as_deref(), Some("当前版本是 2.1"));
    }

    #[test]
    fn returns_none_for_non_signal_message() {
        let result = parse_peer_input_signal("some random error").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parses_signal_wrapped_in_text() {
        let signal = RequestPeerInputSignal {
            target_agent_id: "agent-x".into(),
            question: "帮我检查代码".into(),
            context: None,
        };
        let payload = signal.to_error_message().unwrap();
        let wrapped = format!("Error: tool failed: {payload}");
        let parsed = parse_peer_input_signal(&wrapped)
            .unwrap()
            .expect("should parse signal from wrapped text");
        assert_eq!(parsed.target_agent_id, "agent-x");
    }

    #[test]
    fn rejects_empty_target_agent_id() {
        let input = RequestPeerInputToolInput {
            target_agent_id: "   ".into(),
            question: "some question".into(),
            context: None,
        };
        let result = RequestPeerInputSignal::from_input(input);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_empty_question() {
        let input = RequestPeerInputToolInput {
            target_agent_id: "agent-1".into(),
            question: "  ".into(),
            context: None,
        };
        let result = RequestPeerInputSignal::from_input(input);
        assert!(result.is_err());
    }
}
