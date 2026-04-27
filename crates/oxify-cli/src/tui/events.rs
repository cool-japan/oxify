/// High-level commands derived from raw keyboard events.
#[derive(Debug, Clone)]
pub enum AppCommand {
    Quit,
    SelectNext,
    SelectPrev,
    SelectFirst,
    SelectLast,
    Enter,
    Back,
    Refresh,
    Tab,
    None,
}
