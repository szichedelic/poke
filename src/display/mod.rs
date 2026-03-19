pub mod cli;
pub mod count;

use chrono::{DateTime, Utc};

/// Format a timestamp as a human-readable "Xm ago" / "Xh ago" / "Xs ago" string.
pub fn format_since(since: &DateTime<Utc>) -> String {
    let elapsed = Utc::now().signed_duration_since(*since);
    let secs = elapsed.num_seconds().max(0);

    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn format_since_seconds() {
        let since = Utc::now() - Duration::seconds(30);
        let result = format_since(&since);
        assert!(result.ends_with("s ago"), "got: {}", result);
    }

    #[test]
    fn format_since_minutes() {
        let since = Utc::now() - Duration::minutes(5);
        assert_eq!(format_since(&since), "5m ago");
    }

    #[test]
    fn format_since_hours() {
        let since = Utc::now() - Duration::hours(2);
        assert_eq!(format_since(&since), "2h ago");
    }
}
