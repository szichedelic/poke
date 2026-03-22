use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;
use serde::Deserialize;

use crate::models::{AgentStatus, AgentStatusKind, WaitingType};

/// The JSON payload Claude Code sends to the Notification hook on stdin.
///
/// Actual schema from Claude Code:
/// ```json
/// {
///   "hook_event_name": "Notification",
///   "session_id": "...",
///   "cwd": "...",
///   "message": "Claude is waiting for your input",
///   "title": "Optional title",
///   "notification_type": "idle_prompt"
/// }
/// ```
///
/// Known `notification_type` values:
/// - `idle_prompt` — agent is waiting for user input
/// - `worker_permission_prompt` — agent needs tool permission
/// - `elicitation_complete` — MCP elicitation finished (informational)
/// - `elicitation_response` — MCP elicitation response (informational)
/// - `auth_success` — login succeeded (informational)
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // session_id and cwd are deserialized for completeness but not yet consumed
struct HookInput {
    /// The notification message from Claude Code.
    #[serde(default)]
    message: Option<String>,
    /// Optional title (e.g., "Tool Use: Edit").
    #[serde(default)]
    title: Option<String>,
    /// The type of notification — determines whether the agent needs attention.
    #[serde(default)]
    notification_type: Option<String>,
    /// The Claude Code session ID.
    #[serde(default)]
    session_id: Option<String>,
    /// Working directory of the Claude Code session.
    #[serde(default)]
    cwd: Option<String>,
}

/// Resolve the tmux session name for the current pane.
fn get_tmux_session() -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#S"])
        .output()
        .ok()?;

    if output.status.success() {
        let session = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if session.is_empty() {
            None
        } else {
            Some(session)
        }
    } else {
        None
    }
}

/// Classify the notification into a WaitingType based on `notification_type`.
///
/// Returns `Some(WaitingType)` when the agent needs human attention,
/// `None` for informational notifications (agent is working/doesn't need input).
fn classify_waiting_type(input: &HookInput) -> Option<WaitingType> {
    match input.notification_type.as_deref() {
        // Claude Code real notification types
        Some("idle_prompt") => Some(WaitingType::Question),
        Some("worker_permission_prompt") => Some(WaitingType::Approval),
        // Informational — agent doesn't need attention
        Some("auth_success") | Some("elicitation_complete") | Some("elicitation_response") => None,
        // Legacy/custom types for forward compatibility
        Some("approval") => Some(WaitingType::Approval),
        Some("choice") => Some(WaitingType::Choice),
        Some("question") => Some(WaitingType::Question),
        // Explicit working/resumed signals
        Some("working") | Some("resumed") => None,
        // Unknown type with a message — conservatively treat as needing attention
        Some(_) if input.message.is_some() => Some(WaitingType::Question),
        // Unknown type, no message — informational
        Some(_) => None,
        // No type but has a message — treat as needing attention
        None if input.message.is_some() => Some(WaitingType::Question),
        // No type and no message — nothing to show
        None => None,
    }
}

/// Get the events directory, creating it if needed.
fn events_dir() -> io::Result<PathBuf> {
    let dir = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))?
        .join(".poke")
        .join("events");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Run the hook-notify command: read stdin, resolve context, write event file.
pub fn run() -> Result<(), String> {
    // Read JSON from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("failed to read stdin: {}", e))?;

    let hook_input: HookInput =
        serde_json::from_str(&input).map_err(|e| format!("failed to parse stdin JSON: {}", e))?;

    // Get tmux pane from environment
    let tmux_pane = std::env::var("TMUX_PANE")
        .map_err(|_| "TMUX_PANE not set — are you running inside tmux?".to_string())?;

    // Resolve tmux session
    let tmux_session = get_tmux_session().unwrap_or_else(|| "unknown".to_string());

    // Get parent PID (the agent process that invoked the hook)
    let pid = std::os::unix::process::parent_id();

    let waiting_type = classify_waiting_type(&hook_input);
    let status_kind = if waiting_type.is_some() {
        AgentStatusKind::Waiting
    } else {
        AgentStatusKind::Working
    };

    // Build summary: prefer "title: message" when both present
    let summary = match (&hook_input.title, &hook_input.message) {
        (Some(title), Some(msg)) => Some(format!("{}: {}", title, msg)),
        (None, Some(msg)) => Some(msg.clone()),
        (Some(title), None) => Some(title.clone()),
        (None, None) => None,
    };

    let event = AgentStatus {
        agent: "claude-code".to_string(),
        status: status_kind,
        waiting_type,
        summary,
        tmux_session,
        tmux_pane: tmux_pane.clone(),
        pid: Some(pid),
        since: Utc::now(),
    };

    // Write event file — filename is pane_id with % escaped
    let filename = format!("{}.json", tmux_pane.replace('%', "pct"));
    let dir = events_dir().map_err(|e| format!("failed to create events dir: {}", e))?;
    let event_path = dir.join(&filename);

    let json = serde_json::to_string_pretty(&event)
        .map_err(|e| format!("failed to serialize event: {}", e))?;

    fs::write(&event_path, json).map_err(|e| format!("failed to write event file: {}", e))?;

    Ok(())
}

/// Write an event file to a specific directory (for testing and direct use).
pub fn write_event(events_dir: &std::path::Path, event: &AgentStatus) -> io::Result<()> {
    fs::create_dir_all(events_dir)?;
    let filename = format!("{}.json", event.tmux_pane.replace('%', "pct"));
    let path = events_dir.join(&filename);
    let json = serde_json::to_string_pretty(event).map_err(io::Error::other)?;
    fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WaitingType;
    use tempfile::TempDir;

    fn hook_input(ntype: Option<&str>, message: Option<&str>) -> HookInput {
        HookInput {
            message: message.map(|s| s.to_string()),
            title: None,
            notification_type: ntype.map(|s| s.to_string()),
            session_id: None,
            cwd: None,
        }
    }

    #[test]
    fn classify_question() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("question"), Some("test?"))),
            Some(WaitingType::Question)
        );
    }

    #[test]
    fn classify_approval() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("approval"), Some("allow?"))),
            Some(WaitingType::Approval)
        );
    }

    #[test]
    fn classify_choice() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("choice"), Some("pick one"))),
            Some(WaitingType::Choice)
        );
    }

    #[test]
    fn classify_unknown_with_message_defaults_to_question() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("unknown"), Some("hey"))),
            Some(WaitingType::Question)
        );
    }

    #[test]
    fn classify_no_type_with_message_is_question() {
        assert_eq!(
            classify_waiting_type(&hook_input(None, Some("something"))),
            Some(WaitingType::Question)
        );
    }

    #[test]
    fn classify_working_is_none() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("working"), None)),
            None
        );
    }

    #[test]
    fn classify_resumed_is_none() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("resumed"), None)),
            None
        );
    }

    #[test]
    fn classify_no_type_no_message_is_none() {
        assert_eq!(classify_waiting_type(&hook_input(None, None)), None);
    }

    #[test]
    fn classify_idle_prompt() {
        assert_eq!(
            classify_waiting_type(&hook_input(
                Some("idle_prompt"),
                Some("Claude is waiting for your input")
            )),
            Some(WaitingType::Question)
        );
    }

    #[test]
    fn classify_worker_permission_prompt() {
        assert_eq!(
            classify_waiting_type(&hook_input(
                Some("worker_permission_prompt"),
                Some("agent needs permission for Bash")
            )),
            Some(WaitingType::Approval)
        );
    }

    #[test]
    fn classify_auth_success_is_none() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("auth_success"), Some("login ok"))),
            None
        );
    }

    #[test]
    fn classify_elicitation_complete_is_none() {
        assert_eq!(
            classify_waiting_type(&hook_input(Some("elicitation_complete"), Some("done"))),
            None
        );
    }

    #[test]
    fn parse_real_claude_code_payload() {
        let json = r#"{
            "hook_event_name": "Notification",
            "session_id": "abc-123",
            "cwd": "/home/user/project",
            "message": "Claude is waiting for your input",
            "title": "Input Required",
            "notification_type": "idle_prompt"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.message,
            Some("Claude is waiting for your input".to_string())
        );
        assert_eq!(input.notification_type, Some("idle_prompt".to_string()));
        assert_eq!(input.title, Some("Input Required".to_string()));
        assert_eq!(input.session_id, Some("abc-123".to_string()));
        assert_eq!(input.cwd, Some("/home/user/project".to_string()));
    }

    #[test]
    fn parse_hook_input_minimal() {
        let json = r#"{}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.message, None);
        assert_eq!(input.notification_type, None);
    }

    #[test]
    fn parse_hook_input_extra_fields() {
        let json = r#"{"message": "test", "notification_type": "idle_prompt", "extra": true}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.message, Some("test".to_string()));
        assert_eq!(input.notification_type, Some("idle_prompt".to_string()));
    }

    #[test]
    fn write_event_creates_file() {
        let dir = TempDir::new().unwrap();
        let event = AgentStatus {
            agent: "claude-code".to_string(),
            status: AgentStatusKind::Waiting,
            waiting_type: Some(WaitingType::Question),
            summary: Some("test question".to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: "%42".to_string(),
            pid: Some(12345),
            since: Utc::now(),
        };

        write_event(dir.path(), &event).unwrap();

        let path = dir.path().join("pct42.json");
        assert!(path.exists());

        let contents = fs::read_to_string(&path).unwrap();
        let parsed: AgentStatus = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.agent, "claude-code");
        assert_eq!(parsed.tmux_pane, "%42");
        assert_eq!(parsed.summary, Some("test question".to_string()));
    }

    #[test]
    fn write_event_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let event1 = AgentStatus {
            agent: "claude-code".to_string(),
            status: AgentStatusKind::Waiting,
            waiting_type: Some(WaitingType::Question),
            summary: Some("first".to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: "%42".to_string(),
            pid: Some(12345),
            since: Utc::now(),
        };
        let event2 = AgentStatus {
            status: AgentStatusKind::Working,
            waiting_type: None,
            summary: None,
            ..event1.clone()
        };

        write_event(dir.path(), &event1).unwrap();
        write_event(dir.path(), &event2).unwrap();

        let path = dir.path().join("pct42.json");
        let contents = fs::read_to_string(&path).unwrap();
        let parsed: AgentStatus = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed.status, AgentStatusKind::Working);
    }
}
