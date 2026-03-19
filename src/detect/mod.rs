pub mod scraper;
pub mod structured;

use std::collections::HashMap;

use crate::models::{AgentStatus, AgentStatusKind};

pub trait Detector {
    fn scan(&self) -> Vec<AgentStatus>;
}

/// Aggregates results from multiple detectors with deduplication.
///
/// Dedup key: `tmux_pane`. When both structured and scraping detectors
/// find the same pane, the structured result wins (more reliable data).
/// The aggregator also filters out `status: "working"` entries.
pub struct Aggregator {
    detectors: Vec<Box<dyn Detector>>,
}

impl Aggregator {
    pub fn new() -> Self {
        Self {
            detectors: Vec::new(),
        }
    }

    /// Add a detector. Detectors added first have higher priority in dedup
    /// (structured should be added before scraping).
    pub fn add_detector(&mut self, detector: Box<dyn Detector>) {
        self.detectors.push(detector);
    }

    /// Run all detectors, deduplicate by tmux_pane (first detector wins),
    /// and filter to only waiting agents.
    pub fn scan_waiting(&self) -> Vec<AgentStatus> {
        let mut seen: HashMap<String, AgentStatus> = HashMap::new();

        for detector in &self.detectors {
            for status in detector.scan() {
                // First detector to claim a pane wins (structured added first = higher priority)
                seen.entry(status.tmux_pane.clone()).or_insert(status);
            }
        }

        let mut results: Vec<AgentStatus> = seen
            .into_values()
            .filter(|s| s.status == AgentStatusKind::Waiting)
            .collect();

        // Sort by since (oldest first) for stable output
        results.sort_by_key(|s| s.since);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentStatus, AgentStatusKind, WaitingType};
    use chrono::{Duration, Utc};

    struct MockDetector {
        results: Vec<AgentStatus>,
    }

    impl Detector for MockDetector {
        fn scan(&self) -> Vec<AgentStatus> {
            self.results.clone()
        }
    }

    fn make_status(
        pane: &str,
        agent: &str,
        kind: AgentStatusKind,
        waiting_type: Option<WaitingType>,
        mins_ago: i64,
    ) -> AgentStatus {
        AgentStatus {
            agent: agent.to_string(),
            status: kind,
            waiting_type,
            summary: Some("test".to_string()),
            tmux_session: "dev".to_string(),
            tmux_pane: pane.to_string(),
            pid: Some(12345),
            since: Utc::now() - Duration::minutes(mins_ago),
        }
    }

    #[test]
    fn empty_aggregator_returns_empty() {
        let agg = Aggregator::new();
        assert!(agg.scan_waiting().is_empty());
    }

    #[test]
    fn single_detector_returns_waiting_only() {
        let mut agg = Aggregator::new();
        agg.add_detector(Box::new(MockDetector {
            results: vec![
                make_status("%1", "claude-code", AgentStatusKind::Waiting, Some(WaitingType::Question), 5),
                make_status("%2", "codex", AgentStatusKind::Working, None, 3),
            ],
        }));

        let results = agg.scan_waiting();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tmux_pane, "%1");
    }

    #[test]
    fn structured_wins_over_scraping_for_same_pane() {
        let mut agg = Aggregator::new();

        // Structured detector (added first = higher priority)
        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status(
                "%42",
                "claude-code",
                AgentStatusKind::Waiting,
                Some(WaitingType::Approval),
                5,
            )],
        }));

        // Scraping detector (lower priority)
        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status(
                "%42",
                "claude-code",
                AgentStatusKind::Waiting,
                Some(WaitingType::Question),
                1,
            )],
        }));

        let results = agg.scan_waiting();
        assert_eq!(results.len(), 1);
        // Structured result wins — approval, not question
        assert_eq!(results[0].waiting_type, Some(WaitingType::Approval));
    }

    #[test]
    fn structured_working_suppresses_scraping_waiting() {
        let mut agg = Aggregator::new();

        // Structured says working
        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status("%42", "claude-code", AgentStatusKind::Working, None, 1)],
        }));

        // Scraping says waiting (stale match)
        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status(
                "%42",
                "claude-code",
                AgentStatusKind::Waiting,
                Some(WaitingType::Question),
                1,
            )],
        }));

        let results = agg.scan_waiting();
        // Structured wins with "working", then filtered out
        assert!(results.is_empty());
    }

    #[test]
    fn different_panes_not_deduped() {
        let mut agg = Aggregator::new();

        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status(
                "%1",
                "claude-code",
                AgentStatusKind::Waiting,
                Some(WaitingType::Question),
                5,
            )],
        }));

        agg.add_detector(Box::new(MockDetector {
            results: vec![make_status(
                "%2",
                "codex",
                AgentStatusKind::Waiting,
                Some(WaitingType::Choice),
                3,
            )],
        }));

        let results = agg.scan_waiting();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn results_sorted_by_since_oldest_first() {
        let mut agg = Aggregator::new();
        agg.add_detector(Box::new(MockDetector {
            results: vec![
                make_status("%1", "claude-code", AgentStatusKind::Waiting, Some(WaitingType::Question), 2),
                make_status("%2", "codex", AgentStatusKind::Waiting, Some(WaitingType::Choice), 10),
                make_status("%3", "claude-code", AgentStatusKind::Waiting, Some(WaitingType::Approval), 5),
            ],
        }));

        let results = agg.scan_waiting();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tmux_pane, "%2"); // 10m ago
        assert_eq!(results[1].tmux_pane, "%3"); // 5m ago
        assert_eq!(results[2].tmux_pane, "%1"); // 2m ago
    }
}
