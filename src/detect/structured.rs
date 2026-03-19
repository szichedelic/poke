use std::fs;
use std::path::PathBuf;

use crate::detect::Detector;
use crate::models::AgentStatus;

pub struct StructuredDetector {
    events_dir: PathBuf,
}

impl StructuredDetector {
    pub fn new() -> Self {
        let events_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".poke")
            .join("events");
        Self { events_dir }
    }

    /// Create a detector reading from a custom directory (for testing).
    pub fn with_dir(events_dir: PathBuf) -> Self {
        Self { events_dir }
    }

    /// Check if a process with the given PID is still running.
    fn is_pid_alive(pid: u32) -> bool {
        // kill -0 checks existence without sending a signal
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    /// Read and parse a single event file. Returns None if invalid/unreadable.
    fn read_event(path: &std::path::Path) -> Option<AgentStatus> {
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
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
                    // Invalid file — remove it
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

            results.push(status);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentStatusKind, WaitingType};
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    fn sample_event(status: &str, pid: Option<u32>) -> String {
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
            Utc::now().to_rfc3339(),
        )
    }

    #[test]
    fn reads_valid_event_file() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        // Use current process PID so it's alive
        let pid = std::process::id();
        fs::write(
            events_dir.join("%42.json"),
            sample_event("waiting", Some(pid)),
        )
        .unwrap();

        let detector = StructuredDetector::with_dir(events_dir);
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

        let detector = StructuredDetector::with_dir(events_dir);
        let results = detector.scan();
        // Structured detector returns all statuses; filtering is done by the aggregator/display
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AgentStatusKind::Working);
    }

    #[test]
    fn removes_stale_pid_event() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        let event_path = events_dir.join("%99.json");
        // PID 999999 is very unlikely to exist
        fs::write(&event_path, sample_event("waiting", Some(999999))).unwrap();

        let detector = StructuredDetector::with_dir(events_dir);
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

        let detector = StructuredDetector::with_dir(events_dir);
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
        let detector = StructuredDetector::with_dir(events_dir);
        let results = detector.scan();
        assert_eq!(results.len(), 0);
        // Non-json files should NOT be deleted
        assert!(txt_path.exists());
    }

    #[test]
    fn handles_missing_events_dir() {
        let detector = StructuredDetector::with_dir(PathBuf::from("/nonexistent/path/events"));
        let results = detector.scan();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn keeps_event_without_pid() {
        let dir = TempDir::new().unwrap();
        let events_dir = dir.path().to_path_buf();
        fs::write(
            events_dir.join("%44.json"),
            sample_event("waiting", None),
        )
        .unwrap();

        let detector = StructuredDetector::with_dir(events_dir);
        let results = detector.scan();
        assert_eq!(results.len(), 1);
    }
}
