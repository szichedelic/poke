use chrono::Utc;
use regex::Regex;
use std::process::Command;

use crate::config::{Pattern, ScrapingConfig};
use crate::detect::Detector;
use crate::models::{AgentStatus, AgentStatusKind, WaitingType};

/// A compiled pattern ready for matching.
struct CompiledPattern {
    agent: String,
    waiting_type: WaitingType,
    regex: Regex,
}

/// Tmux pane info parsed from `tmux list-panes`.
struct PaneInfo {
    session_name: String,
    pane_target: String,
    pane_id: String,
}

pub struct TmuxScraper {
    patterns: Vec<CompiledPattern>,
    scraping_config: ScrapingConfig,
}

impl TmuxScraper {
    pub fn new(patterns: &[Pattern], scraping_config: ScrapingConfig) -> Self {
        let compiled = patterns
            .iter()
            .filter_map(|p| {
                let regex = Regex::new(&p.regex).ok()?;
                let waiting_type = match p.waiting_type.as_str() {
                    "question" => WaitingType::Question,
                    "approval" => WaitingType::Approval,
                    "choice" => WaitingType::Choice,
                    _ => return None,
                };
                Some(CompiledPattern {
                    agent: p.agent.clone(),
                    waiting_type,
                    regex,
                })
            })
            .collect();

        Self {
            patterns: compiled,
            scraping_config,
        }
    }

    fn list_panes() -> Vec<PaneInfo> {
        let output = Command::new("tmux")
            .args(["list-panes", "-a", "-F", "#{session_name} #{pane_id}"])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let session_name = parts.next()?.to_string();
                let pane_id = parts.next()?.to_string();
                // pane_target is used for capture-pane; pane_id (e.g. %42) works directly
                Some(PaneInfo {
                    session_name,
                    pane_target: pane_id.clone(),
                    pane_id,
                })
            })
            .collect()
    }

    fn capture_pane(pane_id: &str) -> Option<String> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-p", "-t", pane_id, "-S", "-20"])
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    fn is_session_included(&self, session: &str) -> bool {
        let include = &self.scraping_config.include_sessions;
        let exclude = &self.scraping_config.exclude_sessions;

        if !include.is_empty() {
            return include.iter().any(|s| s == session);
        }
        !exclude.iter().any(|s| s == session)
    }

    fn match_content(&self, content: &str) -> Option<(String, WaitingType, String)> {
        for line in content.lines().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            for pattern in &self.patterns {
                if pattern.regex.is_match(trimmed) {
                    let summary = truncate(trimmed, 80);
                    return Some((
                        pattern.agent.clone(),
                        pattern.waiting_type.clone(),
                        summary,
                    ));
                }
            }
        }
        None
    }
}

impl Detector for TmuxScraper {
    fn scan(&self) -> Vec<AgentStatus> {
        let panes = Self::list_panes();
        let now = Utc::now();
        let mut results = Vec::new();

        for pane in panes {
            if !self.is_session_included(&pane.session_name) {
                continue;
            }

            let content = match Self::capture_pane(&pane.pane_target) {
                Some(c) => c,
                None => continue,
            };

            if let Some((agent, waiting_type, summary)) = self.match_content(&content) {
                results.push(AgentStatus {
                    agent,
                    status: AgentStatusKind::Waiting,
                    waiting_type: Some(waiting_type),
                    summary: Some(summary),
                    tmux_session: pane.session_name,
                    tmux_pane: pane.pane_id,
                    pid: None,
                    since: now,
                });
            }
        }

        results
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.saturating_sub(1);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Pattern;

    fn test_patterns() -> Vec<Pattern> {
        vec![
            Pattern {
                name: "test-approval".to_string(),
                agent: "claude-code".to_string(),
                waiting_type: "approval".to_string(),
                regex: r"Allow.*\?".to_string(),
            },
            Pattern {
                name: "test-question".to_string(),
                agent: "claude-code".to_string(),
                waiting_type: "question".to_string(),
                regex: r"\?\s*$".to_string(),
            },
            Pattern {
                name: "test-choice".to_string(),
                agent: "codex".to_string(),
                waiting_type: "choice".to_string(),
                regex: r"^\s*\d+[\.\)]\s+".to_string(),
            },
        ]
    }

    fn default_scraping_config() -> ScrapingConfig {
        ScrapingConfig {
            include_sessions: Vec::new(),
            exclude_sessions: Vec::new(),
        }
    }

    #[test]
    fn compiles_valid_patterns() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        assert_eq!(scraper.patterns.len(), 3);
    }

    #[test]
    fn skips_invalid_patterns() {
        let patterns = vec![Pattern {
            name: "bad".to_string(),
            agent: "test".to_string(),
            waiting_type: "approval".to_string(),
            regex: r"[invalid".to_string(),
        }];
        let scraper = TmuxScraper::new(&patterns, default_scraping_config());
        assert_eq!(scraper.patterns.len(), 0);
    }

    #[test]
    fn skips_unknown_waiting_type() {
        let patterns = vec![Pattern {
            name: "unknown".to_string(),
            agent: "test".to_string(),
            waiting_type: "unknown_type".to_string(),
            regex: r"test".to_string(),
        }];
        let scraper = TmuxScraper::new(&patterns, default_scraping_config());
        assert_eq!(scraper.patterns.len(), 0);
    }

    #[test]
    fn matches_approval_pattern() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        let content = "Some output\nAllow edit to src/main.rs?\n";
        let result = scraper.match_content(content);
        assert!(result.is_some());
        let (agent, wt, summary) = result.unwrap();
        assert_eq!(agent, "claude-code");
        assert_eq!(wt, WaitingType::Approval);
        assert!(summary.contains("Allow edit"));
    }

    #[test]
    fn matches_question_pattern() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        let content = "Should I create a new file?  \n";
        let result = scraper.match_content(content);
        assert!(result.is_some());
        let (_, wt, _) = result.unwrap();
        assert_eq!(wt, WaitingType::Question);
    }

    #[test]
    fn matches_choice_pattern() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        let content = "Pick an option:\n  1. First option\n";
        let result = scraper.match_content(content);
        assert!(result.is_some());
        let (agent, wt, _) = result.unwrap();
        assert_eq!(agent, "codex");
        assert_eq!(wt, WaitingType::Choice);
    }

    #[test]
    fn no_match_returns_none() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        let content = "Just regular output\nNothing special here\n";
        assert!(scraper.match_content(content).is_none());
    }

    #[test]
    fn matches_last_matching_line_first() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        // Both lines match, but the last non-empty line is checked first (reverse order)
        let content = "Should I do this?\nAllow edit to foo?\n";
        let result = scraper.match_content(content);
        assert!(result.is_some());
        let (_, wt, _) = result.unwrap();
        assert_eq!(wt, WaitingType::Approval);
    }

    #[test]
    fn session_filter_include() {
        let config = ScrapingConfig {
            include_sessions: vec!["dev".to_string()],
            exclude_sessions: Vec::new(),
        };
        let scraper = TmuxScraper::new(&test_patterns(), config);
        assert!(scraper.is_session_included("dev"));
        assert!(!scraper.is_session_included("other"));
    }

    #[test]
    fn session_filter_exclude() {
        let config = ScrapingConfig {
            include_sessions: Vec::new(),
            exclude_sessions: vec!["scratch".to_string(), "music".to_string()],
        };
        let scraper = TmuxScraper::new(&test_patterns(), config);
        assert!(scraper.is_session_included("dev"));
        assert!(!scraper.is_session_included("scratch"));
        assert!(!scraper.is_session_included("music"));
    }

    #[test]
    fn session_filter_no_filters_includes_all() {
        let scraper = TmuxScraper::new(&test_patterns(), default_scraping_config());
        assert!(scraper.is_session_included("anything"));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 80), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let long = "a".repeat(100);
        let result = truncate(&long, 80);
        assert!(result.len() <= 83); // 79 chars + "…" (3 bytes)
        assert!(result.ends_with('…'));
    }
}
