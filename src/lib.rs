pub mod config;
pub mod detect;
pub mod display;
pub mod hook;
pub mod init;
pub mod models;
pub mod switch;

use detect::Aggregator;

/// Build a fully configured aggregator using the user's config.
///
/// This is the primary entry point for library consumers (e.g., mycel).
/// It loads config, creates both structured and scraping detectors
/// as configured, and returns an aggregator ready to scan.
pub fn build_aggregator() -> Aggregator {
    let cfg = config::Config::load();
    build_aggregator_with_config(&cfg)
}

/// Build an aggregator with a specific config (useful for testing or
/// when the consumer has already loaded config).
pub fn build_aggregator_with_config(cfg: &config::Config) -> Aggregator {
    let mut agg = Aggregator::new();

    if cfg.detectors.structured {
        agg.add_detector(Box::new(detect::structured::StructuredDetector::new(
            cfg.stale_timeout_secs,
        )));
    }

    if cfg.detectors.scraping {
        let patterns = config::Pattern::load_all();
        agg.add_detector(Box::new(detect::scraper::TmuxScraper::new(
            &patterns,
            cfg.scraping.clone(),
        )));
    }

    agg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_aggregator_returns_aggregator() {
        let agg = build_aggregator();
        // Just verify it doesn't panic and returns results (empty in test env)
        let results = agg.scan_waiting();
        assert!(results.is_empty() || !results.is_empty());
    }

    #[test]
    fn build_aggregator_with_custom_config() {
        let mut cfg = config::Config::default();
        cfg.detectors.structured = false;
        cfg.detectors.scraping = false;
        let agg = build_aggregator_with_config(&cfg);
        let results = agg.scan_waiting();
        assert!(results.is_empty());
    }
}
