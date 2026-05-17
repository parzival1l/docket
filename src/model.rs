use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub deps: Vec<i64>,
    pub status: String,
    pub priority: i32,
    pub group: Option<String>,
    pub kind: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The closed vocabulary of task kinds. Keeps filtering/UX simple.
pub const KINDS: &[&str] = &["bug", "feature", "chore", "docs", "spike"];

pub fn validate_kind(k: &str) -> Result<()> {
    if KINDS.iter().any(|valid| *valid == k) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "invalid kind `{}` — must be one of: {}",
            k,
            KINDS.join(", ")
        ))
    }
}

/// 3-letter shorthand for dense list views. Falls back to the first 3 chars
/// for kinds we don't know about (forward-compat).
pub fn kind_short(k: &str) -> &str {
    match k {
        "bug" => "bug",
        "feature" => "fea",
        "chore" => "cho",
        "docs" => "doc",
        "spike" => "spi",
        other => {
            let end = other
                .char_indices()
                .nth(3)
                .map(|(i, _)| i)
                .unwrap_or(other.len());
            &other[..end]
        }
    }
}


#[derive(Serialize, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub branch_name: Option<String>,
    pub description: Option<String>,
    pub state: String,
    pub created_at: String,
}

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn fmt_id(id: i64) -> String {
    format!("T-{}", id)
}

pub fn parse_id(s: &str) -> Result<i64> {
    let stripped = s
        .trim()
        .trim_start_matches(['T', 't'])
        .trim_start_matches('-');
    stripped
        .parse::<i64>()
        .with_context(|| format!("invalid task id: {}", s))
}

pub fn parse_deps(input: Option<String>) -> Result<Option<String>> {
    match input {
        None => Ok(None),
        Some(s) => {
            let ids: Result<Vec<i64>> = s
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|x| !x.is_empty())
                .map(parse_id)
                .collect();
            let ids = ids?;
            if ids.is_empty() {
                Ok(None)
            } else {
                Ok(Some(serde_json::to_string(&ids)?))
            }
        }
    }
}

pub fn deps_from_db(s: Option<String>) -> Vec<i64> {
    match s {
        None => vec![],
        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_id_formats_with_t_prefix() {
        assert_eq!(fmt_id(1), "T-1");
        assert_eq!(fmt_id(42), "T-42");
    }

    #[test]
    fn parse_id_accepts_t_prefix_dash() {
        assert_eq!(parse_id("T-7").unwrap(), 7);
        assert_eq!(parse_id("t-7").unwrap(), 7);
    }

    #[test]
    fn parse_id_accepts_bare_int() {
        assert_eq!(parse_id("9").unwrap(), 9);
        assert_eq!(parse_id("  9  ").unwrap(), 9);
    }

    #[test]
    fn parse_id_rejects_garbage() {
        assert!(parse_id("foo").is_err());
        assert!(parse_id("T-").is_err());
        assert!(parse_id("").is_err());
    }

    #[test]
    fn parse_deps_none_returns_none() {
        assert_eq!(parse_deps(None).unwrap(), None);
    }

    #[test]
    fn parse_deps_empty_string_returns_none() {
        assert_eq!(parse_deps(Some("".into())).unwrap(), None);
        assert_eq!(parse_deps(Some("   ".into())).unwrap(), None);
    }

    #[test]
    fn parse_deps_comma_separated() {
        let got = parse_deps(Some("T-3,T-5,9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn parse_deps_whitespace_separated() {
        let got = parse_deps(Some("T-3 5 9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn deps_from_db_none_is_empty() {
        assert!(deps_from_db(None).is_empty());
    }

    #[test]
    fn deps_from_db_parses_json_array() {
        assert_eq!(deps_from_db(Some("[1,2,3]".into())), vec![1, 2, 3]);
    }

    #[test]
    fn deps_from_db_invalid_json_is_empty() {
        assert_eq!(deps_from_db(Some("not-json".into())), Vec::<i64>::new());
    }
}
