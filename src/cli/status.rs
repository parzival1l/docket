use anyhow::{anyhow, Result};

use crate::db::{load_all_tasks, open_db, set_status};
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

pub fn promote(id: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let t = all
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow!("{} not found", fmt_id(id)))?;
    if t.status != "backlog" {
        return Err(anyhow!(
            "{} is not in backlog (status = {}); promote only moves backlog tasks to open",
            fmt_id(id),
            t.status
        ));
    }
    set_status(&conn, id, "open")?;
    println!("{} -> open", fmt_id(id));
    Ok(())
}
