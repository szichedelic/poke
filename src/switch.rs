use std::fmt;
use std::process::Command;

use crate::models::AgentStatus;

#[derive(Debug)]
pub enum SwitchError {
    TmuxNotRunning,
    TargetNotFound(String),
    CommandFailed(String),
}

impl fmt::Display for SwitchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwitchError::TmuxNotRunning => write!(f, "tmux is not running"),
            SwitchError::TargetNotFound(target) => {
                write!(f, "target pane no longer exists: {}", target)
            }
            SwitchError::CommandFailed(msg) => write!(f, "switch failed: {}", msg),
        }
    }
}

impl std::error::Error for SwitchError {}

/// Allowed quick-response values. Only unambiguous, safe responses are permitted.
const ALLOWED_RESPONSES: &[&str] = &["y", "n", "yes", "no"];

/// Build the tmux target string from an AgentStatus.
/// Format: `{session}:{pane}` where pane is the raw pane ID (e.g., %42).
fn build_target(status: &AgentStatus) -> String {
    format!("{}:{}", status.tmux_session, status.tmux_pane)
}

/// Switch the current tmux client to the pane described by `status`.
pub fn switch_to_agent(status: &AgentStatus) -> Result<(), SwitchError> {
    let target = build_target(status);

    let output = Command::new("tmux")
        .args(["switch-client", "-t", &target])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SwitchError::TmuxNotRunning
            } else {
                SwitchError::CommandFailed(e.to_string())
            }
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("can't find")
        || stderr_lower.contains("no such")
        || stderr_lower.contains("not found")
    {
        Err(SwitchError::TargetNotFound(target))
    } else if stderr_lower.contains("no server") || stderr_lower.contains("no current client") {
        Err(SwitchError::TmuxNotRunning)
    } else {
        Err(SwitchError::CommandFailed(stderr.trim().to_string()))
    }
}

/// Validate that a response is allowed for quick-response.
/// Only simple yes/no responses are permitted as a safety guard.
pub fn validate_response(response: &str) -> Result<(), SwitchError> {
    let lower = response.to_lowercase();
    if ALLOWED_RESPONSES.contains(&lower.as_str()) {
        Ok(())
    } else {
        Err(SwitchError::CommandFailed(format!(
            "response {:?} is not allowed — only {} are permitted for safety",
            response,
            ALLOWED_RESPONSES.join(", ")
        )))
    }
}

/// Send a quick response to an agent's pane via `tmux send-keys`.
///
/// Safety guards:
/// - Only approval-type waiting agents can receive quick responses
/// - Only simple yes/no responses are allowed
/// - Sends the response followed by Enter
pub fn send_response(status: &AgentStatus, response: &str) -> Result<(), SwitchError> {
    use crate::models::WaitingType;

    // Safety: only allow responding to approval prompts
    match &status.waiting_type {
        Some(WaitingType::Approval) => {}
        Some(other) => {
            return Err(SwitchError::CommandFailed(format!(
                "quick response only works for approval prompts, not {:?}",
                other
            )));
        }
        None => {
            return Err(SwitchError::CommandFailed(
                "cannot respond to agent with unknown waiting type".to_string(),
            ));
        }
    }

    validate_response(response)?;

    let target = build_target(status);

    // Send the response text followed by Enter
    let output = Command::new("tmux")
        .args(["send-keys", "-t", &target, response, "Enter"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SwitchError::TmuxNotRunning
            } else {
                SwitchError::CommandFailed(e.to_string())
            }
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("can't find")
        || stderr_lower.contains("no such")
        || stderr_lower.contains("not found")
    {
        Err(SwitchError::TargetNotFound(target))
    } else if stderr_lower.contains("no server") || stderr_lower.contains("no current client") {
        Err(SwitchError::TmuxNotRunning)
    } else {
        Err(SwitchError::CommandFailed(stderr.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentStatus, AgentStatusKind, WaitingType};
    use chrono::Utc;

    fn sample_status() -> AgentStatus {
        AgentStatus {
            agent: "claude-code".to_string(),
            status: AgentStatusKind::Waiting,
            waiting_type: Some(WaitingType::Question),
            summary: Some("test".to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: "%42".to_string(),
            pid: Some(12345),
            since: Utc::now(),
        }
    }

    #[test]
    fn build_target_formats_correctly() {
        let status = sample_status();
        assert_eq!(build_target(&status), "dev:%42");
    }

    #[test]
    fn switch_error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(SwitchError::TmuxNotRunning);
        assert_eq!(e.to_string(), "tmux is not running");
    }

    #[test]
    fn switch_error_display() {
        let e = SwitchError::TmuxNotRunning;
        assert_eq!(e.to_string(), "tmux is not running");

        let e = SwitchError::TargetNotFound("dev:%42".to_string());
        assert_eq!(e.to_string(), "target pane no longer exists: dev:%42");

        let e = SwitchError::CommandFailed("something broke".to_string());
        assert_eq!(e.to_string(), "switch failed: something broke");
    }

    #[test]
    fn validate_response_allows_yes_no() {
        assert!(validate_response("y").is_ok());
        assert!(validate_response("n").is_ok());
        assert!(validate_response("yes").is_ok());
        assert!(validate_response("no").is_ok());
        assert!(validate_response("Y").is_ok());
        assert!(validate_response("N").is_ok());
    }

    #[test]
    fn validate_response_rejects_unsafe() {
        assert!(validate_response("rm -rf /").is_err());
        assert!(validate_response("1").is_err());
        assert!(validate_response("maybe").is_err());
        assert!(validate_response("").is_err());
    }

    #[test]
    fn send_response_rejects_non_approval() {
        let status = sample_status(); // has WaitingType::Question
        let result = send_response(&status, "y");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("approval"), "got: {}", err);
    }

    #[test]
    fn send_response_rejects_no_waiting_type() {
        let mut status = sample_status();
        status.waiting_type = None;
        let result = send_response(&status, "y");
        assert!(result.is_err());
    }

    #[test]
    fn send_response_rejects_unsafe_input() {
        let mut status = sample_status();
        status.waiting_type = Some(WaitingType::Approval);
        let result = send_response(&status, "rm -rf /");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not allowed"), "got: {}", err);
    }
}
