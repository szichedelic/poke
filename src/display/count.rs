/// Format the status bar count string.
///
/// Returns `status_format` with `{count}` replaced when count > 0,
/// or `status_empty` when count == 0.
pub fn format_count(count: usize, status_format: &str, status_empty: &str) -> String {
    if count == 0 {
        status_empty.to_string()
    } else {
        status_format.replace("{count}", &count.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_returns_empty() {
        assert_eq!(format_count(0, "● {count}", ""), "");
    }

    #[test]
    fn zero_returns_custom_empty() {
        assert_eq!(format_count(0, "● {count}", "-"), "-");
    }

    #[test]
    fn nonzero_formats_count() {
        assert_eq!(format_count(3, "● {count}", ""), "● 3");
    }

    #[test]
    fn single_agent() {
        assert_eq!(format_count(1, "● {count}", ""), "● 1");
    }

    #[test]
    fn custom_format() {
        assert_eq!(format_count(5, "! {count} waiting", ""), "! 5 waiting");
    }
}
