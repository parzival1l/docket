use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, Task};

use super::print_task_row;

#[derive(Serialize)]
struct BlockedEntry {
    task: Task,
    unmet_deps: Vec<String>,
}

pub fn run(group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let by_id: HashMap<i64, &Task> = all.iter().map(|t| (t.id, t)).collect();

    let mut entries: Vec<BlockedEntry> = vec![];
    for t in &all {
        if t.status != "open" {
            continue;
        }
        if let Some(g) = &group {
            if t.group.as_deref() != Some(g.as_str()) {
                continue;
            }
        }
        let unmet: Vec<String> = t
            .deps
            .iter()
            .filter_map(|d| match by_id.get(d) {
                Some(x) if x.status == "done" => None,
                Some(x) => Some(format!("{} ({})", fmt_id(*d), x.status)),
                None => Some(format!("{} (missing)", fmt_id(*d))),
            })
            .collect();
        if !unmet.is_empty() {
            entries.push(BlockedEntry {
                task: t.clone(),
                unmet_deps: unmet,
            });
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("(no blocked tasks)");
    } else {
        for e in &entries {
            print_task_row(&e.task);
            println!("       blocked by: {}", e.unmet_deps.join(", "));
        }
    }
    Ok(())
}
