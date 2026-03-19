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
    fn switch_error_display() {
        let e = SwitchError::TmuxNotRunning;
        assert_eq!(e.to_string(), "tmux is not running");

        let e = SwitchError::TargetNotFound("dev:%42".to_string());
        assert_eq!(e.to_string(), "target pane no longer exists: dev:%42");

        let e = SwitchError::CommandFailed("something broke".to_string());
        assert_eq!(e.to_string(), "switch failed: something broke");
    }
}
