use serde::Deserialize;

const DEFAULT_PATTERNS_TOML: &str = include_str!("../defaults/patterns.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub agent: String,
    pub waiting_type: String,
    pub regex: String,
}

#[derive(Debug, Deserialize)]
struct PatternsFile {
    #[serde(default)]
    patterns: Vec<Pattern>,
}

impl Pattern {
    /// Load patterns: user patterns from ~/.poke/patterns.toml merged with embedded defaults.
    /// User patterns with the same name override defaults.
    pub fn load_all() -> Vec<Pattern> {
        let mut patterns: Vec<Pattern> = match toml::from_str::<PatternsFile>(DEFAULT_PATTERNS_TOML)
        {
            Ok(f) => f.patterns,
            Err(_) => Vec::new(),
        };

        if let Some(user_path) = dirs::home_dir().map(|h| h.join(".poke").join("patterns.toml")) {
            if let Ok(contents) = std::fs::read_to_string(&user_path) {
                if let Ok(user_file) = toml::from_str::<PatternsFile>(&contents) {
                    for user_pat in user_file.patterns {
                        if let Some(existing) =
                            patterns.iter_mut().find(|p| p.name == user_pat.name)
                        {
                            *existing = user_pat;
                        } else {
                            patterns.push(user_pat);
                        }
                    }
                }
            }
        }

        patterns
    }

    /// Load only the embedded default patterns (no filesystem access).
    pub fn defaults() -> Vec<Pattern> {
        toml::from_str::<PatternsFile>(DEFAULT_PATTERNS_TOML)
            .map(|f| f.patterns)
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    #[serde(default = "default_stale_timeout")]
    pub stale_timeout_secs: u64,
    #[serde(default = "default_status_format")]
    pub status_format: String,
    #[serde(default)]
    pub status_empty: String,
    #[serde(default)]
    pub detectors: DetectorsConfig,
    #[serde(default)]
    pub scraping: ScrapingConfig,
}

#[derive(Debug, Deserialize)]
pub struct DetectorsConfig {
    #[serde(default = "default_true")]
    pub structured: bool,
    #[serde(default = "default_true")]
    pub scraping: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScrapingConfig {
    #[serde(default)]
    pub include_sessions: Vec<String>,
    #[serde(default)]
    pub exclude_sessions: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scan_interval_secs: default_scan_interval(),
            stale_timeout_secs: default_stale_timeout(),
            status_format: default_status_format(),
            status_empty: String::new(),
            detectors: DetectorsConfig::default(),
            scraping: ScrapingConfig::default(),
        }
    }
}

impl Default for DetectorsConfig {
    fn default() -> Self {
        Self {
            structured: true,
            scraping: true,
        }
    }
}

fn default_scan_interval() -> u64 {
    3
}

fn default_stale_timeout() -> u64 {
    300
}

fn default_status_format() -> String {
    "● {count}".to_string()
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load config from ~/.poke/config.toml, returning an error if the file
    /// exists but cannot be parsed. Missing file is not an error (returns defaults).
    pub fn load() -> Result<Self, String> {
        let config_path = match dirs::home_dir() {
            Some(h) => h.join(".poke").join("config.toml"),
            None => return Ok(Config::default()),
        };

        let contents = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return Ok(Config::default()), // File doesn't exist
        };

        toml::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {}", config_path.display(), e))
    }

    /// Load config, silently falling back to defaults on any error.
    /// Use this only where errors must never be surfaced (e.g., tmux status bar).
    pub fn load_or_default() -> Self {
        Config::load().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert_eq!(config.scan_interval_secs, 3);
        assert_eq!(config.stale_timeout_secs, 300);
        assert_eq!(config.status_format, "● {count}");
        assert!(config.status_empty.is_empty());
        assert!(config.detectors.structured);
        assert!(config.detectors.scraping);
    }

    #[test]
    fn default_patterns_load() {
        let patterns = Pattern::defaults();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].name, "claude-code-approval");
        assert_eq!(patterns[0].agent, "claude-code");
        assert_eq!(patterns[0].waiting_type, "approval");
        assert_eq!(patterns[1].name, "claude-code-question");
        assert_eq!(patterns[1].waiting_type, "question");
        assert_eq!(patterns[2].name, "codex-choice");
        assert_eq!(patterns[2].agent, "codex");
        assert_eq!(patterns[2].waiting_type, "choice");
    }

    #[test]
    fn default_patterns_are_valid_regex() {
        let patterns = Pattern::defaults();
        for pat in &patterns {
            regex::Regex::new(&pat.regex)
                .unwrap_or_else(|e| panic!("Pattern '{}' has invalid regex: {}", pat.name, e));
        }
    }

    #[test]
    fn invalid_toml_gives_parse_error() {
        let bad_toml = "scan_interval_secs\nno equals sign here";
        let result = toml::from_str::<Config>(bad_toml);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("expected"),
            "error should describe problem: {}",
            err
        );
    }

    #[test]
    fn load_or_default_returns_defaults() {
        // In test environment, ~/.poke/config.toml likely doesn't exist,
        // so this should return defaults without error.
        let cfg = Config::load_or_default();
        assert_eq!(cfg.scan_interval_secs, default_scan_interval());
    }

    #[test]
    fn config_deserializes_from_toml() {
        let toml_str = r#"
scan_interval_secs = 5
stale_timeout_secs = 600
status_format = "! {count}"
status_empty = "-"

[detectors]
structured = true
scraping = false

[scraping]
exclude_sessions = ["scratch"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.scan_interval_secs, 5);
        assert_eq!(config.stale_timeout_secs, 600);
        assert!(!config.detectors.scraping);
        assert_eq!(config.scraping.exclude_sessions, vec!["scratch"]);
    }
}
