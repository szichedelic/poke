mod config;
mod detect;
mod display;
mod hook;
mod init;
mod models;
mod switch;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "poke", version, about = "Agent attention aggregator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List agents waiting for attention (default when no subcommand given)
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Output a short count for tmux status bar
    Count,
    /// Live-updating TUI showing agent status
    Watch,
    /// Receive a hook notification from an agent (stdin JSON)
    HookNotify,
    /// Set up agent hooks (e.g., Claude Code notification hook)
    Init,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::List { json }) => cmd_list(json),
        Some(Commands::Count) => cmd_count(),
        Some(Commands::Watch) => cmd_watch(),
        Some(Commands::HookNotify) => cmd_hook_notify(),
        Some(Commands::Init) => cmd_init(),
        None => cmd_list(false),
    }
}

fn build_aggregator() -> detect::Aggregator {
    let cfg = config::Config::load();
    let mut agg = detect::Aggregator::new();

    // Structured detector first (higher priority in dedup)
    if cfg.detectors.structured {
        agg.add_detector(Box::new(detect::structured::StructuredDetector::new()));
    }

    if cfg.detectors.scraping {
        let patterns = config::Pattern::load_all();
        agg.add_detector(Box::new(detect::scraper::TmuxScraper::new(
            &patterns,
            cfg.scraping,
        )));
    }

    agg
}

fn cmd_list(json: bool) {
    let agg = build_aggregator();
    let agents = agg.scan_waiting();
    display::cli::run_list(agents, json);
}

fn cmd_count() {
    // Must never print errors to stdout — this runs in tmux status bar.
    // On any failure, output empty string silently.
    let output = std::panic::catch_unwind(|| {
        let cfg = config::Config::load();
        let agg = build_aggregator();
        let count = agg.scan_waiting().len();
        display::count::format_count(count, &cfg.status_format, &cfg.status_empty)
    });

    match output {
        Ok(s) => print!("{}", s),
        Err(_) => {} // Silent failure
    }
}

fn cmd_watch() {
    let cfg = config::Config::load();
    let agg = build_aggregator();
    let interval = std::time::Duration::from_secs(cfg.scan_interval_secs);
    if let Err(e) = display::tui::run(agg, interval) {
        eprintln!("poke watch: {}", e);
        std::process::exit(1);
    }
}

fn cmd_hook_notify() {
    if let Err(e) = hook::run() {
        eprintln!("poke hook-notify: {}", e);
        std::process::exit(1);
    }
}

fn cmd_init() {
    match init::run() {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("poke init: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_parses_no_args() {
        let cli = Cli::parse_from(["poke"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_parses_list() {
        let cli = Cli::parse_from(["poke", "list"]);
        assert!(matches!(cli.command, Some(Commands::List { json: false })));
    }

    #[test]
    fn cli_parses_list_json() {
        let cli = Cli::parse_from(["poke", "list", "--json"]);
        assert!(matches!(cli.command, Some(Commands::List { json: true })));
    }

    #[test]
    fn cli_parses_count() {
        let cli = Cli::parse_from(["poke", "count"]);
        assert!(matches!(cli.command, Some(Commands::Count)));
    }

    #[test]
    fn cli_parses_watch() {
        let cli = Cli::parse_from(["poke", "watch"]);
        assert!(matches!(cli.command, Some(Commands::Watch)));
    }

    #[test]
    fn cli_parses_hook_notify() {
        let cli = Cli::parse_from(["poke", "hook-notify"]);
        assert!(matches!(cli.command, Some(Commands::HookNotify)));
    }

    #[test]
    fn cli_parses_init() {
        let cli = Cli::parse_from(["poke", "init"]);
        assert!(matches!(cli.command, Some(Commands::Init)));
    }
}
