use anyhow::{anyhow, Result};

use crate::db::{open_db, set_status};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String, state: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = set_status(&conn, id, &state)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} -> {}", fmt_id(id), state);
    Ok(())
}

pub fn done(id: String) -> Result<()> {
    run(id, "done".into())
}
