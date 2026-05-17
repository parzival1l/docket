use anyhow::{anyhow, Result};

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String, json: bool) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let t = all
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow!("{} not found", fmt_id(id)))?;
    if json {
        println!("{}", serde_json::to_string_pretty(t)?);
    } else {
        let dep_states: Vec<String> = t
            .deps
            .iter()
            .map(|d| match all.iter().find(|x| x.id == *d) {
                Some(dep) => format!("{} ({})", fmt_id(*d), dep.status),
                None => format!("{} (missing)", fmt_id(*d)),
            })
            .collect();
        println!(
            "{}  {}  [{}]  [{}]  p{}",
            fmt_id(t.id),
            t.title,
            t.status,
            t.kind,
            t.priority
        );
        if let Some(g) = &t.group {
            println!("group: {}", g);
        }
        if !dep_states.is_empty() {
            println!("deps:  {}", dep_states.join(", "));
        }
        if !t.agent_sessions.is_empty() {
            println!("sessions: {}", t.agent_sessions.join(", "));
        }
        println!("created: {}", t.created_at);
        if t.updated_at != t.created_at {
            println!("updated: {}", t.updated_at);
        }
        if let Some(b) = &t.body {
            println!("\n## body\n{}", b);
        }
        if let Some(a) = &t.acceptance {
            println!("\n## acceptance\n{}", a);
        }
    }
    Ok(())
}
