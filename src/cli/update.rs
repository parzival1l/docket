use anyhow::{anyhow, Result};

use crate::db::{get_or_create_group, open_db, update_task_fields, TaskFieldUpdate};
use crate::model::{fmt_id, parse_deps, parse_id};

pub fn run(
    id: String,
    title: Option<String>,
    body: Option<String>,
    acceptance: Option<String>,
    deps: Option<String>,
    priority: Option<i32>,
    group: Option<String>,
) -> Result<()> {
    if title.is_none()
        && body.is_none()
        && acceptance.is_none()
        && deps.is_none()
        && priority.is_none()
        && group.is_none()
    {
        return Err(anyhow!(
            "no fields to update — pass at least one of --title, --body, --acceptance, --deps, --priority, --group"
        ));
    }

    let id = parse_id(&id)?;
    let conn = open_db()?;

    let group_id = match group.as_deref() {
        Some(name) => Some(get_or_create_group(&conn, name)?),
        None => None,
    };
    let deps_json = parse_deps(deps)?;

    let n = update_task_fields(
        &conn,
        id,
        TaskFieldUpdate {
            title: title.as_deref(),
            body: body.as_deref(),
            acceptance: acceptance.as_deref(),
            deps_json: deps_json.as_deref(),
            priority,
            group_id,
        },
    )?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} updated", fmt_id(id));
    Ok(())
}
