use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::fs;

use crate::db::{find_repo_root, init_schema};

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd).ok_or_else(|| anyhow!("not in a git repo"))?;
    let dir = root.join(".docket");
    let db = dir.join("db.sqlite");

    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        println!("created {}", dir.display());
    }

    let conn = Connection::open(&db)?;
    init_schema(&conn)?;
    println!("initialized {}", db.display());

    let gi = root.join(".gitignore");
    let line = ".docket/";
    let needs_append = if gi.exists() {
        let c = fs::read_to_string(&gi)?;
        !c.lines().any(|l| l.trim() == line)
    } else {
        true
    };
    if needs_append {
        let mut content = if gi.exists() {
            fs::read_to_string(&gi)?
        } else {
            String::new()
        };
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(line);
        content.push('\n');
        fs::write(&gi, content)?;
        println!("added `.docket/` to {}", gi.display());
    } else {
        println!(".gitignore already excludes .docket/");
    }
    Ok(())
}
