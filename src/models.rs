use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitingType {
    Question,
    Approval,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusKind {
    Waiting,
    Working,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent: String,
    pub status: AgentStatusKind,
    pub waiting_type: Option<WaitingType>,
    pub summary: Option<String>,
    pub tmux_session: String,
    pub tmux_pane: String,
    pub pid: Option<u32>,
    pub since: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_type_serializes_snake_case() {
        let json = serde_json::to_string(&WaitingType::Question).unwrap();
        assert_eq!(json, "\"question\"");
    }

    #[test]
    fn agent_status_round_trips() {
        let status = AgentStatus {
            agent: "claude-code".to_string(),
            status: AgentStatusKind::Waiting,
            waiting_type: Some(WaitingType::Question),
            summary: Some("Should I create a new file?".to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: "%42".to_string(),
            pid: Some(12345),
            since: Utc::now(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent, "claude-code");
        assert_eq!(deserialized.status, AgentStatusKind::Waiting);
        assert_eq!(deserialized.waiting_type, Some(WaitingType::Question));
    }
}
