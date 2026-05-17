use anyhow::Result;
use std::collections::HashMap;

use crate::db::{load_all_tasks, open_db};
use crate::model::Task;

use super::print_task_row;

pub fn run(group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let by_id: HashMap<i64, &Task> = all.iter().map(|t| (t.id, t)).collect();
    let ready: Vec<&Task> = all
        .iter()
        .filter(|t| t.status == "open")
        .filter(|t| group.as_deref().map_or(true, |g| t.group.as_deref() == Some(g)))
        .filter(|t| {
            t.deps
                .iter()
                .all(|d| by_id.get(d).map_or(true, |x| x.status == "done"))
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&ready)?);
    } else if ready.is_empty() {
        println!("(no ready tasks)");
    } else {
        for t in ready {
            print_task_row(t);
        }
    }
    Ok(())
}
