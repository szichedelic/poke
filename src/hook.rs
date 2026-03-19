use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;
use serde::Deserialize;

use crate::models::{AgentStatus, AgentStatusKind, WaitingType};

/// The JSON payload Claude Code sends to the notification hook on stdin.
#[derive(Debug, Deserialize)]
struct HookInput {
    /// The notification message/summary from Claude Code.
    #[serde(default)]
    message: Option<String>,
    /// The type of notification (e.g., "question", "approval").
    #[serde(default, rename = "type")]
    notification_type: Option<String>,
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

/// Classify the notification type into a WaitingType.
/// Returns `None` when the agent has resumed (working state).
fn classify_waiting_type(input: &HookInput) -> Option<WaitingType> {
    match input.notification_type.as_deref() {
        Some("approval") => Some(WaitingType::Approval),
        Some("choice") => Some(WaitingType::Choice),
        Some("question") => Some(WaitingType::Question),
        // Explicit working/resumed signals
        Some("working") | Some("resumed") => None,
        // No type but has a message — treat as a question (needs attention)
        None if input.message.is_some() => Some(WaitingType::Question),
        // No type and no message — agent resumed, nothing to show
        None => None,
        // Unknown types with a message — treat as question
        Some(_) if input.message.is_some() => Some(WaitingType::Question),
        // Unknown type, no message — treat as working
        Some(_) => None,
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

    let hook_input: HookInput = serde_json::from_str(&input)
        .map_err(|e| format!("failed to parse stdin JSON: {}", e))?;

    // Get tmux pane from environment
    let tmux_pane = std::env::var("TMUX_PANE")
        .map_err(|_| "TMUX_PANE not set — are you running inside tmux?".to_string())?;

    // Resolve tmux session
    let tmux_session = get_tmux_session()
        .unwrap_or_else(|| "unknown".to_string());

    // Get parent PID (the agent process that invoked the hook)
    let pid = std::os::unix::process::parent_id();

    let waiting_type = classify_waiting_type(&hook_input);
    let status_kind = if waiting_type.is_some() {
        AgentStatusKind::Waiting
    } else {
        AgentStatusKind::Working
    };

    let event = AgentStatus {
        agent: "claude-code".to_string(),
        status: status_kind,
        waiting_type,
        summary: hook_input.message,
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

    fs::write(&event_path, json)
        .map_err(|e| format!("failed to write event file: {}", e))?;

    Ok(())
}

/// Write an event file to a specific directory (for testing and direct use).
pub fn write_event(events_dir: &std::path::Path, event: &AgentStatus) -> io::Result<()> {
    fs::create_dir_all(events_dir)?;
    let filename = format!("{}.json", event.tmux_pane.replace('%', "pct"));
    let path = events_dir.join(&filename);
    let json = serde_json::to_string_pretty(event)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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
            notification_type: ntype.map(|s| s.to_string()),
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
        assert_eq!(
            classify_waiting_type(&hook_input(None, None)),
            None
        );
    }

    #[test]
    fn parse_hook_input_full() {
        let json = r#"{"message": "Should I edit this?", "type": "approval"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.message, Some("Should I edit this?".to_string()));
        assert_eq!(input.notification_type, Some("approval".to_string()));
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
        let json = r#"{"message": "test", "type": "question", "extra": true}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.message, Some("test".to_string()));
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
