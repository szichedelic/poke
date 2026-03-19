use std::io::{self, BufRead, IsTerminal, Write};

use crate::display::format_since;
use crate::models::{AgentStatus, AgentStatusKind};
use crate::switch;

/// Filter to only waiting agents.
fn filter_waiting(agents: Vec<AgentStatus>) -> Vec<AgentStatus> {
    agents
        .into_iter()
        .filter(|a| a.status == AgentStatusKind::Waiting)
        .collect()
}

/// Format the table header and rows, returning the full table string.
fn format_table(agents: &[AgentStatus]) -> String {
    if agents.is_empty() {
        return "No agents waiting for attention.".to_string();
    }

    // Compute column widths
    let mut w_agent: usize = 5; // "Agent"
    let mut w_waiting: usize = 11; // "Waiting For"
    let mut w_context: usize = 7; // "Context"
    let mut w_since: usize = 5; // "Since"

    let rows: Vec<(String, String, String, String)> = agents
        .iter()
        .map(|a| {
            let agent = a.agent.clone();
            let waiting = a
                .waiting_type
                .as_ref()
                .map(|w| format!("{:?}", w).to_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            let context = a.summary.clone().unwrap_or_default();
            let since = format_since(&a.since);

            w_agent = w_agent.max(agent.len());
            w_waiting = w_waiting.max(waiting.len());
            w_context = w_context.max(context.len()).min(40);
            w_since = w_since.max(since.len());

            (agent, waiting, context, since)
        })
        .collect();

    let num_width = agents.len().to_string().len().max(1);

    let mut out = String::new();

    // Header
    out.push_str(&format!(
        " {:<nw$} │ {:<aw$} │ {:<ww$} │ {:<cw$} │ {:<sw$}\n",
        "#",
        "Agent",
        "Waiting For",
        "Context",
        "Since",
        nw = num_width,
        aw = w_agent,
        ww = w_waiting,
        cw = w_context,
        sw = w_since,
    ));

    // Separator
    out.push_str(&format!(
        "─{:─<nw$}─┼─{:─<aw$}─┼─{:─<ww$}─┼─{:─<cw$}─┼─{:─<sw$}─\n",
        "",
        "",
        "",
        "",
        "",
        nw = num_width,
        aw = w_agent,
        ww = w_waiting,
        cw = w_context,
        sw = w_since,
    ));

    // Rows
    for (i, (agent, waiting, context, since)) in rows.iter().enumerate() {
        let ctx_display = if context.len() > 40 {
            let mut end = 39;
            while end > 0 && !context.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}…", &context[..end])
        } else {
            context.clone()
        };

        out.push_str(&format!(
            " {:<nw$} │ {:<aw$} │ {:<ww$} │ {:<cw$} │ {:<sw$}\n",
            i + 1,
            agent,
            waiting,
            ctx_display,
            since,
            nw = num_width,
            aw = w_agent,
            ww = w_waiting,
            cw = w_context,
            sw = w_since,
        ));
    }

    out
}

/// Run the CLI list command.
/// If `json` is true, output JSON and return.
/// Otherwise, print a table and optionally prompt for selection (if TTY).
pub fn run_list(agents: Vec<AgentStatus>, json: bool) {
    let waiting = filter_waiting(agents);

    if json {
        let json_str = serde_json::to_string_pretty(&waiting).unwrap_or_else(|_| "[]".to_string());
        println!("{}", json_str);
        return;
    }

    let table = format_table(&waiting);
    print!("{}", table);

    if waiting.is_empty() {
        return;
    }

    if !io::stdout().is_terminal() {
        return;
    }

    // Interactive selection
    loop {
        print!("\nSelect [1-{}, q to quit]: ", waiting.len());
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.eq_ignore_ascii_case("q") || input.is_empty() {
            break;
        }

        match input.parse::<usize>() {
            Ok(n) if n >= 1 && n <= waiting.len() => {
                let agent = &waiting[n - 1];
                match switch::switch_to_agent(agent) {
                    Ok(()) => {
                        println!("Switched to {} ({})", agent.tmux_session, agent.tmux_pane);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            _ => {
                eprintln!("Invalid selection. Enter 1-{} or q to quit.", waiting.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentStatus, AgentStatusKind, WaitingType};
    use chrono::{Duration, Utc};

    fn make_agent(agent: &str, waiting_type: WaitingType, summary: &str, mins_ago: i64) -> AgentStatus {
        AgentStatus {
            agent: agent.to_string(),
            status: AgentStatusKind::Waiting,
            waiting_type: Some(waiting_type),
            summary: Some(summary.to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: "%42".to_string(),
            pid: Some(12345),
            since: Utc::now() - Duration::minutes(mins_ago),
        }
    }

    #[test]
    fn filter_waiting_removes_working() {
        let agents = vec![
            make_agent("claude-code", WaitingType::Question, "test", 1),
            AgentStatus {
                agent: "codex".to_string(),
                status: AgentStatusKind::Working,
                waiting_type: None,
                summary: None,
                tmux_session: "dev".to_string(),
                tmux_pane: "%43".to_string(),
                pid: None,
                since: Utc::now(),
            },
        ];
        let result = filter_waiting(agents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agent, "claude-code");
    }

    #[test]
    fn format_table_empty() {
        let table = format_table(&[]);
        assert_eq!(table, "No agents waiting for attention.");
    }

    #[test]
    fn format_table_single_row() {
        let agents = vec![make_agent("claude-code", WaitingType::Approval, "wants to edit src/main.rs", 3)];
        let table = format_table(&agents);
        assert!(table.contains("Agent"));
        assert!(table.contains("Waiting For"));
        assert!(table.contains("claude-code"));
        assert!(table.contains("approval"));
        assert!(table.contains("wants to edit src/main.rs"));
        assert!(table.contains("3m ago"));
    }

    #[test]
    fn format_table_multiple_rows() {
        let agents = vec![
            make_agent("claude-code", WaitingType::Approval, "edit file", 3),
            make_agent("codex", WaitingType::Choice, "pick option", 12),
        ];
        let table = format_table(&agents);
        assert!(table.contains(" 1 │"));
        assert!(table.contains(" 2 │"));
    }

    #[test]
    fn format_table_truncates_long_context() {
        let long_ctx = "a".repeat(80);
        let agents = vec![make_agent("claude-code", WaitingType::Question, &long_ctx, 1)];
        let table = format_table(&agents);
        assert!(table.contains("…"));
    }

    #[test]
    fn json_output_is_valid() {
        let agents = vec![make_agent("claude-code", WaitingType::Question, "test?", 1)];
        let json = serde_json::to_string_pretty(&agents).unwrap();
        let parsed: Vec<AgentStatus> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].agent, "claude-code");
    }
}
