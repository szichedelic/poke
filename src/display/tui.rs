use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::detect::Aggregator;
use crate::display::format_since;
use crate::models::AgentStatus;
use crate::switch;

pub fn run(aggregator: Aggregator, scan_interval: Duration) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = run_loop(&mut terminal, aggregator, scan_interval);

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    aggregator: Aggregator,
    scan_interval: Duration,
) -> io::Result<()> {
    let mut table_state = TableState::default();
    let mut agents: Vec<AgentStatus> = Vec::new();
    let mut last_scan = Instant::now() - scan_interval; // Force immediate first scan
    let mut status_msg: Option<String> = None;

    loop {
        // Scan if interval elapsed
        if last_scan.elapsed() >= scan_interval {
            agents = aggregator.scan_waiting();
            last_scan = Instant::now();

            // Keep selection in bounds
            if agents.is_empty() {
                table_state.select(None);
            } else if table_state.selected().is_none() {
                table_state.select(Some(0));
            } else if let Some(sel) = table_state.selected() {
                if sel >= agents.len() {
                    table_state.select(Some(agents.len() - 1));
                }
            }
        }

        // Draw
        terminal.draw(|frame| {
            render_ui(frame, &agents, &mut table_state, status_msg.as_deref());
        })?;

        // Handle input with timeout (poll until next scan)
        let timeout = scan_interval.saturating_sub(last_scan.elapsed());
        if event::poll(timeout.max(Duration::from_millis(50)))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                status_msg = None;

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !agents.is_empty() {
                            let i = table_state.selected().unwrap_or(0);
                            table_state.select(Some((i + 1) % agents.len()));
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if !agents.is_empty() {
                            let i = table_state.selected().unwrap_or(0);
                            table_state.select(Some(if i == 0 { agents.len() - 1 } else { i - 1 }));
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(i) = table_state.selected() {
                            if let Some(agent) = agents.get(i) {
                                match switch::switch_to_agent(agent) {
                                    Ok(()) => return Ok(()),
                                    Err(e) => {
                                        status_msg = Some(format!("Switch failed: {}", e));
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        // Force rescan
                        last_scan = Instant::now() - scan_interval;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn render_ui(
    frame: &mut Frame,
    agents: &[AgentStatus],
    table_state: &mut TableState,
    status_msg: Option<&str>,
) {
    let area = frame.area();

    if agents.is_empty() {
        let block = Block::default().title(" poke watch ").borders(Borders::ALL);
        let text = ratatui::widgets::Paragraph::new("No agents waiting for attention.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, area);
        return;
    }

    // Build table rows
    let rows: Vec<Row> = agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let waiting = a
                .waiting_type
                .as_ref()
                .map(|w| format!("{:?}", w).to_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            let context = a.summary.clone().unwrap_or_default();
            let ctx_display = if context.chars().count() > 40 {
                let truncated: String = context.chars().take(39).collect();
                format!("{}…", truncated)
            } else {
                context
            };
            let since = format_since(&a.since);

            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(a.agent.clone()),
                Cell::from(waiting),
                Cell::from(ctx_display),
                Cell::from(a.tmux_session.clone()),
                Cell::from(since),
            ])
        })
        .collect();

    let header = Row::new(vec![
        "#",
        "Agent",
        "Waiting For",
        "Context",
        "Session",
        "Since",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let widths = [
        Constraint::Length(3),
        Constraint::Length(12),
        Constraint::Length(11),
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(8),
    ];

    // Reserve space for status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(" poke watch ").borders(Borders::ALL))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(table, chunks[0], table_state);

    // Status bar
    let status_text = status_msg.unwrap_or("↑/↓ navigate · Enter switch · r refresh · q quit");
    let status =
        ratatui::widgets::Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, chunks[1]);
}
