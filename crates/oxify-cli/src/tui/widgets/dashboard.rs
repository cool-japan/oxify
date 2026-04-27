use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::tui::app::{AppState, Mode};

/// Render the Dashboard view: a list of recent executions with status colouring.
pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let items: Vec<ListItem> = state
        .executions
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let color = status_color(&e.status);
            let marker = if i == state.selected && matches!(state.mode, Mode::Dashboard) {
                "▶ "
            } else {
                "  "
            };
            let id_short: String = e.id.chars().take(8).collect();
            let wf_short: String = e.workflow_id.chars().take(8).collect();
            let text = format!(
                "{}{} [{}] wf={} started={}",
                marker, id_short, e.status, wf_short, e.started_at,
            );
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Recent Executions "),
    );
    f.render_widget(list, area);
}

fn status_color(status: &str) -> Color {
    match status {
        "running" => Color::Green,
        "failed" => Color::Red,
        "completed" => Color::Cyan,
        _ => Color::White,
    }
}
