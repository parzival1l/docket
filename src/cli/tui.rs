use anyhow::Result;

use crate::model::fmt_id;

pub fn run() -> Result<()> {
    if let Some(req) = crate::tui::run_tui()? {
        crate::cli::start::run(fmt_id(req.id), req.tmux)?;
    }
    Ok(())
}
