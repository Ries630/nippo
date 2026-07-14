//! Read-only interactive dashboard over a rebuilt ledger. Feature-gated behind
//! `tui`; the headless `nippo ledger` path never touches this module. Nothing
//! here mutates ledger data — it only renders and navigates.

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Sparkline, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::ledger::{self, RebuildOutcome, Signal};

/// Enter the alternate screen / raw mode, run the event loop, and always
/// restore the terminal — including on the error path.
pub(crate) fn run(outcome: &RebuildOutcome) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, outcome);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, outcome: &RebuildOutcome) -> Result<()> {
    // These aggregates are derived once — the ledger never changes here.
    let series = ledger::new_rule_series(&outcome.ledger);
    let recurring = ledger::recurring_rules(&outcome.ledger);
    let report_count = outcome.ledger.reports.len();
    let mut selected: usize = 0;

    loop {
        terminal.draw(|frame| draw(frame, outcome, &series, &recurring, selected))?;

        // Poll with a timeout so we don't busy-spin between key presses.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // With no reports there is nothing to browse: any key exits.
        if report_count == 0 {
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Down | KeyCode::Char('j') => {
                if selected + 1 < report_count {
                    selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                selected = selected.saturating_sub(1);
            }
            _ => {}
        }
    }
}

fn draw(
    frame: &mut Frame,
    outcome: &RebuildOutcome,
    series: &[u64],
    recurring: &[(String, usize)],
    selected: usize,
) {
    let area = frame.area();

    if outcome.ledger.reports.is_empty() {
        let msg = Paragraph::new("no reports to display — press any key to quit")
            .block(Block::default().borders(Borders::ALL).title("nippo ledger"));
        frame.render_widget(msg, area);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(8),
            Constraint::Length(1),
        ])
        .split(area);

    draw_top_bar(frame, rows[0], outcome);
    draw_middle(frame, rows[1], outcome, selected);
    draw_bottom(frame, rows[2], series, recurring);
    draw_footer(frame, rows[3]);
}

fn draw_top_bar(frame: &mut Frame, area: Rect, outcome: &RebuildOutcome) {
    let (label, color) = signal_style(outcome.signal);
    let reports = outcome.ledger.reports.len();
    let rules = outcome.ledger.known_rules.len();
    let line = Line::from(vec![
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(format!(
            "{reports} reports folded · {rules} cumulative rules · streak: "
        )),
        Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]);
    let para =
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title("nippo ledger"));
    frame.render_widget(para, area);
}

fn draw_middle(frame: &mut Frame, area: Rect, outcome: &RebuildOutcome, selected: usize) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    draw_reports(frame, cols[0], outcome, selected);
    draw_detail(frame, cols[1], outcome, selected);
}

fn draw_reports(frame: &mut Frame, area: Rect, outcome: &RebuildOutcome, selected: usize) {
    let items: Vec<ListItem> = outcome
        .ledger
        .reports
        .iter()
        .map(|r| {
            let label = r.date.clone().unwrap_or_else(|| r.report.clone());
            ListItem::new(format!(
                "{label} | new={} | reseen={}",
                r.new_rules.len(),
                r.reseen_rules.len()
            ))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Reports"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    let last = outcome.ledger.reports.len().saturating_sub(1);
    state.select(Some(selected.min(last)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame, area: Rect, outcome: &RebuildOutcome, selected: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Unclear points");
    let reports = &outcome.ledger.reports;
    let idx = selected.min(reports.len().saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    if let Some(report) = reports.get(idx) {
        if report.points.is_empty() {
            lines.push(Line::from("（詰まりなし）"));
        } else {
            for (i, p) in report.points.iter().enumerate() {
                if i > 0 {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(vec![
                    Span::styled(
                        "Issue: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(p.issue.clone()),
                ]));
                if !p.cause.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Cause: ", Style::default().fg(Color::Yellow)),
                        Span::raw(p.cause.clone()),
                    ]));
                }
                if !p.rule.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Rule: ", Style::default().fg(Color::Green)),
                        Span::raw(p.rule.clone()),
                    ]));
                }
            }
        }
    }
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn draw_bottom(frame: &mut Frame, area: Rect, series: &[u64], recurring: &[(String, usize)]) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("new rules / report"),
        )
        .data(series.iter().copied())
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(sparkline, cols[0]);

    let block = Block::default().borders(Borders::ALL).title("Recurring");
    if recurring.is_empty() {
        let para = Paragraph::new("（再出現なし）").block(block);
        frame.render_widget(para, cols[1]);
    } else {
        let items: Vec<ListItem> = recurring
            .iter()
            .map(|(rule, count)| ListItem::new(format!("×{count}  {rule}")))
            .collect();
        let list = List::new(items).block(block);
        frame.render_widget(list, cols[1]);
    }
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit · "),
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" move"),
    ]))
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, area);
}

/// Signal → (badge label, color). Converged = green, Diverged = red,
/// Continue = yellow.
fn signal_style(signal: Signal) -> (&'static str, Color) {
    match signal {
        Signal::Converged => ("CONVERGED", Color::Green),
        Signal::Diverged => ("DIVERGED", Color::Red),
        Signal::Continue => ("CONTINUE", Color::Yellow),
    }
}
