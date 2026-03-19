# Poke Implementation Plan

## Phase 1 — Core Detection + CLI
- [x] #1 — Initialize Rust project with clap CLI skeleton
- [x] #2 — Define core data models
- [x] #3 — Implement config and pattern loading
- [x] #7 — Add default patterns for Claude Code and Codex
- [x] #4 — Implement Detector trait and tmux scraper
- [x] #6 — Implement tmux switch-client switching
- [x] #5 — Implement CLI list mode with interactive select

## Phase 2 — Structured Detection + Hooks
- [x] #8 — Implement structured event file reader
- [x] #9 — Implement hook-notify subcommand
- [x] #10 — Implement poke init for Claude Code hook setup
- [x] #11 — Implement aggregator with deduplication

## Phase 3 — Status Line + Watch Mode
- [x] #12 — Implement poke count for tmux status bar
- [x] #13 — Implement poke watch TUI mode
- [ ] #14 — Implement stale event cleanup

## Phase 4+
- [ ] #15 — Mycel integration
- [ ] #16 — Quick response
