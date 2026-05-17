use anyhow::Result;

use crate::db::{link_session, open_db, session_task, unlink_session};
use crate::model::{fmt_id, parse_id};

pub fn link(task_id: String, session_id: String) -> Result<()> {
    let task_id = parse_id(&task_id)?;
    let conn = open_db()?;
    link_session(&conn, task_id, &session_id)?;
    println!("linked {} -> {}", session_id, fmt_id(task_id));
    Ok(())
}

pub fn unlink(session_id: String) -> Result<()> {
    let conn = open_db()?;
    let n = unlink_session(&conn, &session_id)?;
    if n == 0 {
        println!("session {} was not linked", session_id);
    } else {
        println!("unlinked {}", session_id);
    }
    Ok(())
}

pub fn show(session_id: String, json: bool) -> Result<()> {
    let conn = open_db()?;
    let task = session_task(&conn, &session_id)?;
    if json {
        let body = match task {
            Some(id) => serde_json::json!({
                "session_id": session_id,
                "task": fmt_id(id),
            }),
            None => serde_json::json!({
                "session_id": session_id,
                "task": serde_json::Value::Null,
            }),
        };
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        match task {
            Some(id) => println!("{} -> {}", session_id, fmt_id(id)),
            None => println!("{} -> (not linked)", session_id),
        }
    }
    Ok(())
}
