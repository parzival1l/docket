use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::Duration;

pub mod app;
pub mod filters;
pub mod keybindings;
pub mod screens;
pub mod widgets;

use app::App;

/// What the TUI was holding when it quit. At most one variant is populated.
#[derive(Debug, Default)]
pub struct TuiExit {
    pub start: Option<app::StartRequest>,
    pub open_session: Option<app::OpenSessionRequest>,
}

/// RAII guard that restores the terminal to a usable state when dropped,
/// even if the event loop panics. Without this, a panic during render or
/// event handling leaves the shell in raw mode + alt screen.
struct TerminalGuard {
    inner: Option<Terminal<CrosstermBackend<Stdout>>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen).context("enter alt screen")?;
        let backend = CrosstermBackend::new(out);
        let terminal = Terminal::new(backend).context("create terminal")?;
        Ok(Self {
            inner: Some(terminal),
        })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        self.inner.as_mut().expect("terminal taken")
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run_tui() -> Result<TuiExit> {
    let mut app = App::new()?;
    let mut guard = TerminalGuard::new()?;

    loop {
        guard.terminal().draw(|f| app.render(f))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(TuiExit {
        start: app.pending_start,
        open_session: app.pending_open_session,
    })
}
