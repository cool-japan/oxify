use std::collections::VecDeque;
use std::io::stdout;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};

use super::{
    client::{ExecutionSummary, TuiApiClient, WorkflowSummary},
    events::AppCommand,
};

/// The active view shown to the user.
#[derive(Debug, Clone)]
pub enum Mode {
    Dashboard,
    Workflows,
    Logs,
}

/// All mutable state for the TUI application.
pub struct AppState {
    pub mode: Mode,
    pub selected: usize,
    pub workflows: Vec<WorkflowSummary>,
    pub executions: Vec<ExecutionSummary>,
    /// Ring buffer of log lines, capped at 500 entries.
    pub log_buf: VecDeque<String>,
    pub status_msg: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Dashboard,
            selected: 0,
            workflows: Vec::new(),
            executions: Vec::new(),
            log_buf: VecDeque::with_capacity(512),
            status_msg: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// The top-level TUI application.
pub struct TuiApp {
    pub state: AppState,
    api: TuiApiClient,
    tick_ms: u64,
}

impl TuiApp {
    pub fn new(api_url: String, tick_ms: u64) -> Self {
        Self {
            state: AppState::new(),
            api: TuiApiClient::new(api_url),
            tick_ms,
        }
    }

    /// Entry point: initialise the terminal, run the event loop, then restore the terminal.
    pub async fn run(api_url: String, tick_ms: u64) -> Result<()> {
        let mut app = Self::new(api_url, tick_ms);
        app.state.status_msg =
            Some("Press 'q' to quit | Tab: switch view | r: refresh | j/k: navigate".to_string());

        // Initial data fetch before drawing the first frame.
        app.refresh_data().await;

        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;

        let result = app.event_loop(&mut terminal).await;

        // Always restore terminal state, even on error.
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);

        result
    }

    /// The main event loop.
    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        loop {
            terminal.draw(|f| self.draw(f))?;

            if event::poll(Duration::from_millis(self.tick_ms))? {
                if let Event::Key(key) = event::read()? {
                    let cmd = self.handle_key(key);
                    match cmd {
                        AppCommand::Quit => break,
                        AppCommand::Refresh => self.refresh_data().await,
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// Refresh workflows and executions from the API, then append a log entry.
    pub async fn refresh_data(&mut self) {
        if let Ok(workflows) = self.api.list_workflows().await {
            self.state.workflows = workflows;
        }
        if let Ok(executions) = self.api.list_executions().await {
            self.state.executions = executions;
        }
        let ts = chrono::Utc::now().format("%H:%M:%S").to_string();
        self.push_log(format!(
            "[{}] Data refreshed ({} workflows, {} executions)",
            ts,
            self.state.workflows.len(),
            self.state.executions.len(),
        ));
    }

    /// Push a line into the ring buffer, evicting the oldest entry once capacity is exceeded.
    fn push_log(&mut self, line: String) {
        self.state.log_buf.push_back(line);
        if self.state.log_buf.len() > 500 {
            self.state.log_buf.pop_front();
        }
    }

    /// Translate a raw key event into a high-level `AppCommand`.
    pub fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                AppCommand::Quit
            }
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                let max = self.current_list_len().saturating_sub(1);
                self.state.selected = (self.state.selected + 1).min(max);
                AppCommand::SelectNext
            }
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                self.state.selected = self.state.selected.saturating_sub(1);
                AppCommand::SelectPrev
            }
            (KeyCode::Char('g'), _) => {
                self.state.selected = 0;
                AppCommand::SelectFirst
            }
            (KeyCode::Char('G'), _) => {
                self.state.selected = self.current_list_len().saturating_sub(1);
                AppCommand::SelectLast
            }
            (KeyCode::Tab, _) => {
                self.state.mode = match self.state.mode {
                    Mode::Dashboard => Mode::Workflows,
                    Mode::Workflows => Mode::Logs,
                    Mode::Logs => Mode::Dashboard,
                };
                self.state.selected = 0;
                AppCommand::Tab
            }
            (KeyCode::Enter, _) => AppCommand::Enter,
            (KeyCode::Esc, _) | (KeyCode::Backspace, _) => AppCommand::Back,
            (KeyCode::Char('r'), _) => AppCommand::Refresh,
            _ => AppCommand::None,
        }
    }

    /// Returns the number of selectable items in the current view.
    fn current_list_len(&self) -> usize {
        match self.state.mode {
            Mode::Dashboard => self.state.executions.len(),
            Mode::Workflows => self.state.workflows.len(),
            Mode::Logs => self.state.log_buf.len(),
        }
    }

    /// Render a single frame.
    pub fn draw(&self, f: &mut Frame) {
        use ratatui::layout::{Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title bar
                Constraint::Min(0),    // main content
                Constraint::Length(1), // status bar
            ])
            .split(f.area());

        super::widgets::draw_title_bar(f, chunks[0], &self.state.mode);

        match self.state.mode {
            Mode::Dashboard => super::widgets::dashboard::draw(f, chunks[1], &self.state),
            Mode::Workflows => super::widgets::workflow_list::draw(f, chunks[1], &self.state),
            Mode::Logs => super::widgets::logs::draw(f, chunks[1], &self.state),
        }

        super::widgets::draw_status_bar(f, chunks[2], &self.state);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_keybinding_quit_returns_quit() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        let cmd = app.handle_key(make_key(KeyCode::Char('q')));
        assert!(matches!(cmd, AppCommand::Quit));
    }

    #[test]
    fn test_navigation_j_increments_selected() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.mode = Mode::Workflows;
        app.state.workflows = vec![
            crate::tui::client::WorkflowSummary {
                id: "1".into(),
                name: "A".into(),
                status: "active".into(),
                created_at: "".into(),
            },
            crate::tui::client::WorkflowSummary {
                id: "2".into(),
                name: "B".into(),
                status: "active".into(),
                created_at: "".into(),
            },
        ];
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.state.selected, 1);
    }

    #[test]
    fn test_navigation_k_decrements_clamped_at_zero() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.selected = 0;
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.state.selected, 0); // no underflow
    }

    #[test]
    fn test_tab_cycles_mode() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        assert!(matches!(app.state.mode, Mode::Dashboard));
        app.handle_key(make_key(KeyCode::Tab));
        assert!(matches!(app.state.mode, Mode::Workflows));
        app.handle_key(make_key(KeyCode::Tab));
        assert!(matches!(app.state.mode, Mode::Logs));
        app.handle_key(make_key(KeyCode::Tab));
        assert!(matches!(app.state.mode, Mode::Dashboard));
    }

    #[test]
    fn test_log_ring_buffer_caps_at_500() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        for i in 0..600_usize {
            app.state.log_buf.push_back(format!("line {}", i));
            if app.state.log_buf.len() > 500 {
                app.state.log_buf.pop_front();
            }
        }
        assert_eq!(app.state.log_buf.len(), 500);
    }

    #[test]
    fn test_render_dashboard_smoke() {
        use ratatui::{backend::TestBackend, Terminal};
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        let app = TuiApp::new("http://localhost:8080".to_string(), 250);
        terminal
            .draw(|f| app.draw(f))
            .expect("draw should not panic");
    }

    #[test]
    fn test_ctrl_c_returns_quit() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let cmd = app.handle_key(key);
        assert!(matches!(cmd, AppCommand::Quit));
    }

    #[test]
    fn test_g_jumps_to_first() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.mode = Mode::Workflows;
        app.state.workflows = vec![
            crate::tui::client::WorkflowSummary {
                id: "1".into(),
                name: "A".into(),
                status: "active".into(),
                created_at: "".into(),
            },
            crate::tui::client::WorkflowSummary {
                id: "2".into(),
                name: "B".into(),
                status: "active".into(),
                created_at: "".into(),
            },
        ];
        app.state.selected = 1;
        app.handle_key(make_key(KeyCode::Char('g')));
        assert_eq!(app.state.selected, 0);
    }

    #[test]
    fn test_shift_g_jumps_to_last() {
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.mode = Mode::Workflows;
        app.state.workflows = vec![
            crate::tui::client::WorkflowSummary {
                id: "1".into(),
                name: "A".into(),
                status: "active".into(),
                created_at: "".into(),
            },
            crate::tui::client::WorkflowSummary {
                id: "2".into(),
                name: "B".into(),
                status: "active".into(),
                created_at: "".into(),
            },
            crate::tui::client::WorkflowSummary {
                id: "3".into(),
                name: "C".into(),
                status: "active".into(),
                created_at: "".into(),
            },
        ];
        app.handle_key(make_key(KeyCode::Char('G')));
        assert_eq!(app.state.selected, 2);
    }

    #[test]
    fn test_render_workflows_smoke() {
        use ratatui::{backend::TestBackend, Terminal};
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.mode = Mode::Workflows;
        app.state.workflows = vec![crate::tui::client::WorkflowSummary {
            id: "wf-001".into(),
            name: "Test Workflow".into(),
            status: "active".into(),
            created_at: "2024-01-01T00:00:00Z".into(),
        }];
        terminal
            .draw(|f| app.draw(f))
            .expect("draw should not panic");
    }

    #[test]
    fn test_render_logs_smoke() {
        use ratatui::{backend::TestBackend, Terminal};
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal creation failed");
        let mut app = TuiApp::new("http://localhost:8080".to_string(), 250);
        app.state.mode = Mode::Logs;
        app.state.log_buf.push_back("test log line".into());
        terminal
            .draw(|f| app.draw(f))
            .expect("draw should not panic");
    }
}
