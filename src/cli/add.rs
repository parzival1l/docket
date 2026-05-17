use anyhow::Result;

use crate::db::{self, get_or_create_group, open_db, NewTask};
use crate::model::{fmt_id, parse_deps, validate_kind};

pub fn run(
    title: String,
    body: Option<String>,
    acceptance: Option<String>,
    deps: Option<String>,
    priority: Option<i32>,
    group: Option<String>,
    kind: Option<String>,
) -> Result<()> {
    let kind = kind.unwrap_or_else(|| "feature".to_string());
    validate_kind(&kind)?;
    let conn = open_db()?;
    let group_id = match group {
        Some(name) => Some(get_or_create_group(&conn, &name)?),
        None => None,
    };
    let deps_json = parse_deps(deps)?;
    let priority = priority.unwrap_or(2);
    let id = db::insert_task(
        &conn,
        NewTask {
            title: &title,
            body: body.as_deref(),
            acceptance: acceptance.as_deref(),
            deps_json: deps_json.as_deref(),
            priority,
            group_id,
            kind: &kind,
        },
    )?;
    println!("{} added", fmt_id(id));
    Ok(())
}
