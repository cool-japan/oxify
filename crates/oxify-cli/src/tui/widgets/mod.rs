pub mod dashboard;
pub mod execution_detail;
pub mod logs;
pub mod workflow_list;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use super::app::{AppState, Mode};

/// Render the top-most title bar showing the active view and key hints.
pub fn draw_title_bar(f: &mut Frame, area: Rect, mode: &Mode) {
    let mode_label = match mode {
        Mode::Dashboard => "Dashboard",
        Mode::Workflows => "Workflows",
        Mode::Logs => "Logs",
    };
    let title = format!(
        " OxiFY TUI  [{}]  (q=quit, Tab=switch view, r=refresh, j/k=navigate) ",
        mode_label
    );
    let p = Paragraph::new(title).style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(p, area);
}

/// Render the bottom status bar.
pub fn draw_status_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let msg = state.status_msg.as_deref().unwrap_or("Ready");
    let p = Paragraph::new(msg).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(p, area);
}
