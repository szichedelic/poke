mod config;
mod detect;
mod display;
mod hook;
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

fn cmd_list(json: bool) {
    let cfg = config::Config::load();
    let patterns = config::Pattern::load_all();
    let scraper = detect::scraper::TmuxScraper::new(&patterns, cfg.scraping);
    let agents = detect::Detector::scan(&scraper);
    display::cli::run_list(agents, json);
}

fn cmd_count() {
    // Output empty string when no agents waiting (or on any error)
    // Must never print errors — this runs in tmux status bar
}

fn cmd_watch() {
    eprintln!("Watch mode not yet implemented.");
}

fn cmd_hook_notify() {
    if let Err(e) = hook::run() {
        eprintln!("poke hook-notify: {}", e);
        std::process::exit(1);
    }
}

fn cmd_init() {
    eprintln!("init not yet implemented.");
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
