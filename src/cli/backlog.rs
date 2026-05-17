use anyhow::Result;

use crate::db::{load_all_tasks, open_db};
use crate::model::Task;

use super::print_task_row;

pub fn run(group: Option<String>, kind: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let filtered: Vec<&Task> = all
        .iter()
        .filter(|t| t.status == "backlog")
        .filter(|t| group.as_deref().map_or(true, |g| t.group.as_deref() == Some(g)))
        .filter(|t| kind.as_deref().map_or(true, |k| t.kind == k))
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else if filtered.is_empty() {
        println!("(no backlog tasks)");
    } else {
        for t in filtered {
            print_task_row(t);
        }
    }
    Ok(())
}
