use anyhow::{anyhow, Result};

use crate::prompts::{PROMPT_COMMIT, PROMPT_CREATE_TASK, PROMPT_PR, PROMPT_TDD};

pub fn run(name: String) -> Result<()> {
    let body = match name.as_str() {
        "tdd-pursuit" | "tdd" => PROMPT_TDD,
        "create-task" | "create" => PROMPT_CREATE_TASK,
        "commit" => PROMPT_COMMIT,
        "pr" => PROMPT_PR,
        other => {
            return Err(anyhow!(
                "unknown prompt: {}\nknown: tdd-pursuit, create-task, commit, pr",
                other
            ))
        }
    };
    print!("{}", body);
    Ok(())
}
