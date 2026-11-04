//! Local preview: detects runnable dev-server scripts in a workspace's
//! package.json. The script is started in a real terminal session (visible
//! output, user-controlled) — never silently.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevScript {
    pub name: String,
    pub command: String,
    /// Likely a dev server (vs. any other script).
    pub server: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub has_package_json: bool,
    pub package_manager: String,
    pub scripts: Vec<DevScript>,
}

const SERVER_NAMES: [&str; 6] = ["dev", "start", "serve", "preview", "develop", "storybook"];
const SERVER_HINTS: [&str; 14] = [
    "vite", "next dev", "next start", "astro dev", "nuxt", "remix", "webpack serve", "webpack-dev-server",
    "react-scripts start", "http-server", "serve ", "svelte-kit", "parcel", "storybook",
];

pub fn package_manager(root: &Path, pkg: &Value) -> String {
    if let Some(pm) = pkg.get("packageManager").and_then(Value::as_str) {
        let name = pm.split('@').next().unwrap_or("");
        if matches!(name, "npm" | "pnpm" | "yarn" | "bun") {
            return name.into();
        }
    }
    for (file, pm) in [("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn"), ("bun.lockb", "bun"), ("bun.lock", "bun")] {
        if root.join(file).is_file() {
            return pm.into();
        }
    }
    "npm".into()
}

pub fn detect(root: &Path) -> AppResult<PreviewInfo> {
    let path = root.join("package.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(PreviewInfo { has_package_json: false, package_manager: "npm".into(), scripts: vec![] });
    };
    let pkg: Value = serde_json::from_str(&text).map_err(|e| AppError::Invalid(format!("package.json inválido: {e}")))?;
    let mut scripts: Vec<DevScript> = pkg
        .get("scripts")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|c| (k, c)))
                .map(|(k, c)| {
                    // "vitest" is a test runner, not the vite dev server.
                    let probe = format!("{} ", c.replace("vitest", ""));
                    DevScript {
                        name: k.clone(),
                        command: c.to_string(),
                        server: SERVER_NAMES.contains(&k.as_str()) || SERVER_HINTS.iter().any(|h| probe.contains(h)),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let rank = |s: &DevScript| SERVER_NAMES.iter().position(|n| *n == s.name).unwrap_or(SERVER_NAMES.len() + usize::from(!s.server));
    scripts.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.name.cmp(&b.name)));
    Ok(PreviewInfo { has_package_json: true, package_manager: package_manager(root, &pkg), scripts })
}

/// Validates that `script` exists and returns `(pm, ["run", script])`.
pub fn command_for(root: &Path, script: &str) -> AppResult<(String, Vec<String>)> {
    let info = detect(root)?;
    if !info.scripts.iter().any(|s| s.name == script) {
        return Err(AppError::NotFound(format!("script “{script}” não existe no package.json")));
    }
    Ok((info.package_manager, vec!["run".into(), script.into()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scripts_and_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"build":"tsc","test":"vitest","dev":"vite --port 4000","storybook":"storybook dev","start":"node server.js","web":"next dev"}}"#,
        )
        .unwrap();
        let info = detect(dir.path()).unwrap();
        assert!(info.has_package_json);
        assert_eq!(info.package_manager, "npm");
        let names: Vec<&str> = info.scripts.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["dev", "start", "storybook", "web", "build", "test"]);
        assert!(info.scripts.iter().find(|s| s.name == "web").unwrap().server);
        assert!(!info.scripts.iter().find(|s| s.name == "build").unwrap().server);
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect(dir.path()).unwrap().package_manager, "pnpm");
        assert_eq!(command_for(dir.path(), "dev").unwrap(), ("pnpm".to_string(), vec!["run".to_string(), "dev".to_string()]));
        assert!(command_for(dir.path(), "rm -rf /").is_err());
    }

    #[test]
    fn package_manager_field_wins_and_missing_package_json_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        let pkg: Value = serde_json::json!({"packageManager":"bun@1.2.0"});
        assert_eq!(package_manager(dir.path(), &pkg), "bun");
        assert_eq!(package_manager(dir.path(), &serde_json::json!({})), "yarn");
        let empty = tempfile::tempdir().unwrap();
        let info = detect(empty.path()).unwrap();
        assert!(!info.has_package_json && info.scripts.is_empty());
        std::fs::write(empty.path().join("package.json"), "{oops").unwrap();
        assert!(detect(empty.path()).is_err());
    }
}
