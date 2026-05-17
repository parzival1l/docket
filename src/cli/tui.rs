use anyhow::Result;

use crate::model::fmt_id;

pub fn run() -> Result<()> {
    let exit = crate::tui::run_tui()?;
    if let Some(req) = exit.start {
        crate::cli::start::run(fmt_id(req.id), req.delivery)?;
    }
    if let Some(req) = exit.open_session {
        crate::cli::session::open(req.session_id, true)?;
    }
    Ok(())
}
