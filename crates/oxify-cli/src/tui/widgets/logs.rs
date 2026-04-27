use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::tui::app::AppState;

/// Render the Logs view: most-recent lines at the bottom, oldest at the top.
pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    // Visible height minus the two border lines.
    let visible = area.height.saturating_sub(2) as usize;

    // Collect the most-recent `visible` entries from the ring buffer (newest first),
    // then reverse so the newest entry ends up at the bottom of the list.
    let tail: Vec<ListItem> = state
        .log_buf
        .iter()
        .rev()
        .take(visible)
        .map(|line| ListItem::new(line.as_str()).style(Style::default().fg(Color::White)))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let list = List::new(tail).block(Block::default().borders(Borders::ALL).title(" Logs "));
    f.render_widget(list, area);
}
