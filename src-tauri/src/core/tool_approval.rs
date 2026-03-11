use anyhow::{anyhow, Error};
use serde::{Deserialize, Serialize};

use crate::core::permissions::APPROVAL_REQUIRED_PREFIX;

const APPROVAL_REQUEST_MARKER: &str = "[[NEXTCHAT_APPROVAL_REQUEST]]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingToolApprovalRequest {
    pub tool_id: String,
    pub tool_name: String,
    pub input: String,
}

pub fn annotate_approval_request_error(
    error: Error,
    request: &PendingToolApprovalRequest,
) -> Error {
    if !error.to_string().contains(APPROVAL_REQUIRED_PREFIX) {
        return error;
    }

    match serde_json::to_string(request) {
        Ok(payload) => anyhow!("{APPROVAL_REQUEST_MARKER}{payload}\n{error}"),
        Err(_) => error,
    }
}

pub fn parse_pending_approval_request(error_message: &str) -> Option<PendingToolApprovalRequest> {
    let start = error_message.find(APPROVAL_REQUEST_MARKER)? + APPROVAL_REQUEST_MARKER.len();
    let rest = &error_message[start..];
    let (payload, remainder) = rest.split_once('\n')?;
    if !remainder.contains(APPROVAL_REQUIRED_PREFIX) {
        return None;
    }
    serde_json::from_str(payload).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_approval_request_error, parse_pending_approval_request, PendingToolApprovalRequest,
    };
    use anyhow::anyhow;

    #[test]
    fn approval_request_metadata_round_trips() {
        let request = PendingToolApprovalRequest {
            tool_id: "Bash".into(),
            tool_name: "Bash".into(),
            input: r#"{"command":"ls"}"#.into(),
        };

        let error = annotate_approval_request_error(
            anyhow!("approval required: tool 'Bash' needs explicit approval"),
            &request,
        );

        assert_eq!(
            parse_pending_approval_request(&error.to_string()),
            Some(request)
        );
    }

    #[test]
    fn non_approval_errors_are_not_annotated() {
        let request = PendingToolApprovalRequest {
            tool_id: "Bash".into(),
            tool_name: "Bash".into(),
            input: r#"{"command":"ls"}"#.into(),
        };

        let error = annotate_approval_request_error(anyhow!("tool failed"), &request);

        assert!(parse_pending_approval_request(&error.to_string()).is_none());
    }
}
