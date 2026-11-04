//! App settings: a typed allowlist of keys stored as JSON values.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde_json::Value;

use super::now_ms;
use crate::error::{AppError, AppResult};

#[derive(Clone, Copy)]
enum Kind {
    Bool,
    OptString,
}

const KEYS: &[(&str, Kind)] = &[
    ("onboarding.completed", Kind::Bool),
    ("session.restore", Kind::Bool),
    ("session.lastWorkspaceId", Kind::OptString),
    ("ui.mascot", Kind::Bool),
    ("ui.reduceMotion", Kind::Bool),
    ("workspace.defaultDir", Kind::OptString),
    ("voice.locale", Kind::OptString),
    ("voice.onDevice", Kind::Bool),
    ("voice.speak", Kind::Bool),
    ("voice.autoSend", Kind::Bool),
];

pub fn defaults() -> HashMap<String, Value> {
    HashMap::from([
        ("onboarding.completed".into(), Value::Bool(false)),
        ("session.restore".into(), Value::Bool(true)),
        ("session.lastWorkspaceId".into(), Value::Null),
        ("ui.mascot".into(), Value::Bool(true)),
        ("ui.reduceMotion".into(), Value::Bool(false)),
        ("workspace.defaultDir".into(), Value::Null),
        ("voice.locale".into(), Value::Null),
        ("voice.onDevice".into(), Value::Bool(false)),
        ("voice.speak".into(), Value::Bool(false)),
        ("voice.autoSend".into(), Value::Bool(false)),
    ])
}

fn validate(key: &str, value: &Value) -> AppResult<()> {
    let kind = KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, kind)| *kind)
        .ok_or_else(|| AppError::Invalid(format!("unknown setting: {key}")))?;
    let ok = match kind {
        Kind::Bool => value.is_boolean(),
        Kind::OptString => value.is_null() || value.as_str().is_some_and(|s| s.len() <= 4096),
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::Invalid(format!("invalid value for setting {key}")))
    }
}

pub fn get_all(conn: &Connection) -> AppResult<HashMap<String, Value>> {
    let mut out = defaults();
    let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        // Ignore keys no longer in the allowlist and unparseable values.
        if let (true, Ok(v)) = (out.contains_key(&k), serde_json::from_str::<Value>(&v)) {
            out.insert(k, v);
        }
    }
    Ok(out)
}

pub fn set(conn: &Connection, key: &str, value: &Value) -> AppResult<()> {
    validate(key, value)?;
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, serde_json::to_string(value)?, now_ms()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    #[test]
    fn defaults_then_override() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            assert_eq!(get_all(c)?["ui.mascot"], json!(true));
            set(c, "ui.mascot", &json!(false))?;
            set(c, "session.lastWorkspaceId", &json!("abc"))?;
            let all = get_all(c)?;
            assert_eq!(all["ui.mascot"], json!(false));
            assert_eq!(all["session.lastWorkspaceId"], json!("abc"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rejects_unknown_keys_and_wrong_types() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            assert!(set(c, "nope", &json!(true)).is_err());
            assert!(set(c, "ui.mascot", &json!("yes")).is_err());
            assert!(set(c, "workspace.defaultDir", &json!(42)).is_err());
            Ok(())
        })
        .unwrap();
    }
}
