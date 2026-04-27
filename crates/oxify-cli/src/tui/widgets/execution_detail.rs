use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render the Execution Detail view.
///
/// Full per-node result display is a future enhancement; this widget shows a
/// placeholder until that work is completed.
#[allow(dead_code)]
pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let content = if state.executions.is_empty() {
        "No execution selected.\n\nNavigate to the Dashboard view and press Enter to inspect an execution.".to_string()
    } else if let Some(exec) = state.executions.get(state.selected) {
        let duration = exec
            .duration_ms
            .map(|ms| format!("{}ms", ms))
            .unwrap_or_else(|| "in progress".to_string());
        format!(
            "Execution ID : {}\nWorkflow     : {}\nStatus       : {}\nStarted      : {}\nDuration     : {}\n\n(Per-node detail view coming in a future release.)",
            exec.id, exec.workflow_id, exec.status, exec.started_at, duration,
        )
    } else {
        "No execution selected.".to_string()
    };

    let p = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Execution Detail "),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}
