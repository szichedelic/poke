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
- [x] #14 — Implement stale event cleanup

## Phase 4+
- [x] #15 — Mycel integration
- [x] #16 — Quick response

## Fixes
- [x] #19 — SwitchError should implement std::error::Error
- [x] #18 — Hook should write 'working' status when agent resumes
- [x] #20 — Surface config parse errors in non-count commands
- [x] #21 — Fix poke init hook format to match Claude Code settings schema
- [x] #22 — Expose poke as a library crate for mycel consumption
- [x] #23 — Hook should detect notification type from Claude Code payload
