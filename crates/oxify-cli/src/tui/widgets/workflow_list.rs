use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::tui::app::{AppState, Mode};

/// Render the Workflows list view.
pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let items: Vec<ListItem> = state
        .workflows
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let color = workflow_status_color(&w.status);
            let is_selected = i == state.selected && matches!(state.mode, Mode::Workflows);
            let marker = if is_selected { "▶ " } else { "  " };
            let id_short: String = w.id.chars().take(8).collect();
            let text = format!(
                "{}{} {:30} [{}] created={}",
                marker, id_short, w.name, w.status, w.created_at,
            );
            let style = if is_selected {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Workflows "));
    f.render_widget(list, area);
}

fn workflow_status_color(status: &str) -> Color {
    match status {
        "active" => Color::Green,
        "inactive" | "disabled" => Color::DarkGray,
        "error" => Color::Red,
        _ => Color::White,
    }
}
