use std::fs;
use std::path::PathBuf;

use chrono::Utc;

use crate::detect::Detector;
use crate::models::AgentStatus;

pub struct StructuredDetector {
    events_dir: PathBuf,
    stale_timeout_secs: u64,
}

impl StructuredDetector {
    pub fn new(stale_timeout_secs: u64) -> Self {
        let events_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".poke")
            .join("events");
        Self {
            events_dir,
            stale_timeout_secs,
        }
    }

    /// Create a detector reading from a custom directory (for testing).
    pub fn with_dir(events_dir: PathBuf, stale_timeout_secs: u64) -> Self {
        Self {
            events_dir,
            stale_timeout_secs,
        }
    }

    /// Check if a process with the given PID is still running.
    fn is_pid_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    /// Read and parse a single event file. Returns None if invalid/unreadable.
    fn read_event(path: &std::path::Path) -> Option<AgentStatus> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Check if an event is older than the stale timeout.
    fn is_stale(&self, status: &AgentStatus) -> bool {
        let elapsed = Utc::now().signed_duration_since(status.since);
        elapsed.num_seconds() > self.stale_timeout_secs as i64
    }
}

impl Detector for StructuredDetector {
    fn scan(&self) -> Vec<AgentStatus> {
        let entries = match fs::read_dir(&self.events_dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let status = match Self::read_event(&path) {
                Some(s) => s,
                None => {
                    let _ = fs::remove_file(&path);
                    continue;
                }
            };

            // Stale PID check: if pid is set and process is gone, remove the file
            if let Some(pid) = status.pid {
                if !Self::is_pid_alive(pid) {
                    let _ = fs::remove_file(&path);
                    continue;
                }
            }

            // Stale timeout fallback: delete files older than stale_timeout_secs
            // regardless of PID status (handles PID recycling edge case)
            if self.is_stale(&status) {
                let _ = fs::remove_file(&path);
                continue;
            }

            results.push(status);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentStatusKind, WaitingType};
    use chrono::{Duration, Utc};
    use std::fs;
    use tempfile::TempDir;

    fn sample_event(status: &str, pid: Option<u32>) -> String {
        sample_event_with_since(status, pid, &Utc::now().to_rfc3339())
    }

    fn sample_event_with_since(status: &str, pid: Option<u32>, since: &str) -> String {
        let waiting_type = if status == "waiting" {
            r#""question""#
        } else {
            "null"
        };
        format!(
            r#"{{
                "agent": "claude-code",
                "status": "{}",
                "waiting_type": {},
                "summary": "test summary",
                "tmux_session": "dev",
                "tmux_pane": "%42",
                "pid": {},
                "since": "{}"
            }}"#,
            status,
            waiting_type,
            pid.map(|p| p.to_string())
                .unwrap_or_else(|| "null".to_string()),
            since,
        )
    }

    #[test]
    fn reads_valid_event_file() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let pid = std::process::id();
        fs::write(
            events_dir.join("%42.json"),
            sample_event("waiting", Some(pid)),
        )
        .unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent, "claude-code");
        assert_eq!(results[0].status, AgentStatusKind::Waiting);
        assert_eq!(results[0].waiting_type, Some(WaitingType::Question));
    }

    #[test]
    fn includes_working_status() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let pid = std::process::id();
        fs::write(
            events_dir.join("%43.json"),
            sample_event("working", Some(pid)),
        )
        .unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AgentStatusKind::Working);
    }

    #[test]
    fn removes_stale_pid_event() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let event_path = events_dir.join("%99.json");
        fs::write(&event_path, sample_event("waiting", Some(999999))).unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        assert!(!event_path.exists(), "stale event file should be deleted");
    }

    #[test]
    fn removes_invalid_json_file() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let event_path = events_dir.join("%50.json");
        fs::write(&event_path, "not valid json!!!").unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        assert!(!event_path.exists(), "invalid event file should be deleted");
    }

    #[test]
    fn skips_non_json_files() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        fs::write(events_dir.join("readme.txt"), "not an event").unwrap();

        let txt_path = events_dir.join("readme.txt");
        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        assert!(txt_path.exists());
    }

    #[test]
    fn handles_missing_events_dir() {
        let detector = StructuredDetector::with_dir(PathBuf::from("/nonexistent/path/events"), 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn keeps_event_without_pid() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        fs::write(events_dir.join("%44.json"), sample_event("waiting", None)).unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn removes_event_past_stale_timeout() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let event_path = events_dir.join("%45.json");
        // Event from 10 minutes ago, with alive PID
        let old_since = (Utc::now() - Duration::seconds(600)).to_rfc3339();
        let pid = std::process::id();
        fs::write(
            &event_path,
            sample_event_with_since("waiting", Some(pid), &old_since),
        )
        .unwrap();

        // Timeout of 300s — event at 600s is stale
        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        assert!(
            !event_path.exists(),
            "stale timeout event should be deleted"
        );
    }

    #[test]
    fn keeps_event_within_stale_timeout() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        // Event from 1 minute ago, with alive PID
        let recent_since = (Utc::now() - Duration::seconds(60)).to_rfc3339();
        let pid = std::process::id();
        fs::write(
            events_dir.join("%46.json"),
            sample_event_with_since("waiting", Some(pid), &recent_since),
        )
        .unwrap();

        // Timeout of 300s — event at 60s is still fresh
        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn stale_timeout_removes_no_pid_old_events() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let event_path = events_dir.join("%47.json");
        // No PID, but old event
        let old_since = (Utc::now() - Duration::seconds(600)).to_rfc3339();
        fs::write(
            &event_path,
            sample_event_with_since("waiting", None, &old_since),
        )
        .unwrap();

        let detector = StructuredDetector::with_dir(events_dir, 300);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        assert!(!event_path.exists());
    }
}
