use anyhow::{anyhow, Result};

use crate::db::{delete_task, open_db};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = delete_task(&conn, id)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} deleted", fmt_id(id));
    Ok(())
}
