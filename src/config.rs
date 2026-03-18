use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
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

impl Default for ScrapingConfig {
    fn default() -> Self {
        Self {
            include_sessions: Vec::new(),
            exclude_sessions: Vec::new(),
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
    pub fn load() -> Self {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".poke").join("config.toml"));

        if let Some(path) = config_path {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&contents) {
                    return config;
                }
            }
        }
        Config::default()
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
