//! Portable bundles (`*.forge.json`) to share squads and recipes.
//!
//! Bundles are untrusted external content: they are size-capped, strictly
//! validated, never executed, always previewed before import, imported with
//! origin = "import", and any "full" permission is downgraded to "edit".

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{recipes, squads};
use crate::agents::profiles::{self, ProfileInput};
use crate::error::{AppError, AppResult};

pub const FORMAT: &str = "forge.bundle";
pub const VERSION: u32 = 1;
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BundleProfile {
    pub ref_id: String,
    pub profile: ProfileInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BundleSquad {
    pub ref_id: String,
    pub squad: squads::SquadInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub profiles: Vec<BundleProfile>,
    #[serde(default)]
    pub squad: Option<BundleSquad>,
    #[serde(default)]
    pub recipe: Option<recipes::RecipeInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfile {
    pub name: String,
    pub provider_id: String,
    pub permission: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub kind: String,
    pub name: String,
    pub profiles: Vec<PreviewProfile>,
    pub squad: Option<String>,
    pub recipe: Option<String>,
    pub warnings: Vec<String>,
    /// The normalized bundle to send back for import.
    pub bundle: Bundle,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub profiles: usize,
    pub squad_id: Option<String>,
    pub recipe_id: Option<String>,
}

fn is_builtin(id: &str) -> bool {
    id.starts_with("builtin:")
}

// ── Export ────────────────────────────────────────────────────────────────

fn custom_profiles(conn: &Connection, ids: &[String]) -> AppResult<Vec<BundleProfile>> {
    let mut out: Vec<BundleProfile> = Vec::new();
    for id in ids {
        if is_builtin(id) || out.iter().any(|p| &p.ref_id == id) {
            continue;
        }
        let p = profiles::get(conn, id)?;
        out.push(BundleProfile {
            ref_id: p.id.clone(),
            profile: ProfileInput {
                name: p.name,
                role: p.role,
                description: p.description,
                provider_id: p.provider_id,
                model: p.model,
                instructions: p.instructions,
                permission: p.permission,
                max_turns: p.max_turns,
            },
        });
    }
    Ok(out)
}

fn squad_entry(conn: &Connection, id: &str) -> AppResult<(BundleSquad, Vec<String>)> {
    let s = squads::get(conn, id)?;
    let ids = s.members.iter().map(|m| m.profile_id.clone()).collect();
    Ok((
        BundleSquad {
            ref_id: s.id.clone(),
            squad: squads::SquadInput { name: s.name, description: s.description, members: s.members, rules: s.rules, flow: s.flow, default_mode: s.default_mode },
        },
        ids,
    ))
}

pub fn export_squad(conn: &Connection, id: &str) -> AppResult<Bundle> {
    let (squad, ids) = squad_entry(conn, id)?;
    Ok(Bundle { format: FORMAT.into(), version: VERSION, kind: "squad".into(), name: squad.squad.name.clone(), profiles: custom_profiles(conn, &ids)?, squad: Some(squad), recipe: None })
}

pub fn export_recipe(conn: &Connection, id: &str) -> AppResult<Bundle> {
    let r = recipes::get(conn, id)?;
    let mut ids: Vec<String> = r.plan.iter().flatten().filter_map(|t| t.profile_id.clone()).collect();
    let squad = match &r.squad_id {
        Some(sid) => {
            let (s, member_ids) = squad_entry(conn, sid)?;
            ids.extend(member_ids);
            Some(s)
        }
        None => None,
    };
    Ok(Bundle { format: FORMAT.into(), version: VERSION, kind: "recipe".into(), name: r.name.clone(), profiles: custom_profiles(conn, &ids)?, squad, recipe: Some(recipes::to_input(&r)) })
}

// ── Parse & preview ───────────────────────────────────────────────────────

pub fn parse(text: &str) -> AppResult<Bundle> {
    if text.len() > MAX_BYTES {
        return Err(AppError::Invalid("arquivo grande demais para um bundle (máx. 1 MB)".into()));
    }
    let b: Bundle = serde_json::from_str(text).map_err(|e| AppError::Invalid(format!("bundle inválido: {e}")))?;
    if b.format != FORMAT || b.version != VERSION {
        return Err(AppError::Invalid("formato ou versão de bundle não suportados".into()));
    }
    match b.kind.as_str() {
        "squad" if b.squad.is_some() && b.recipe.is_none() => {}
        "recipe" if b.recipe.is_some() => {}
        _ => return Err(AppError::Invalid("tipo de bundle inválido".into())),
    }
    if b.profiles.len() > 30 {
        return Err(AppError::Invalid("bundle com perfis demais".into()));
    }
    Ok(b)
}

/// Normalizes (downgrades "full") and summarizes a bundle for user review.
pub fn preview(conn: &Connection, mut b: Bundle) -> AppResult<Preview> {
    let mut warnings = Vec::new();
    for p in &mut b.profiles {
        crate::providers::def(&p.profile.provider_id).map_err(|_| AppError::Invalid(format!("provider desconhecido no perfil {}", p.profile.name)))?;
        if p.profile.permission == "full" {
            p.profile.permission = "edit".into();
            warnings.push(format!("O perfil “{}” pedia acesso total — foi rebaixado para edição.", p.profile.name));
        }
    }
    let refs: Vec<&str> = b.profiles.iter().map(|p| p.ref_id.as_str()).collect();
    let check_profile = |id: &str| -> AppResult<()> {
        if is_builtin(id) {
            profiles::get(conn, id).map(|_| ()).map_err(|_| AppError::Invalid(format!("o bundle usa o perfil padrão {id}, que não existe nesta versão")))
        } else if refs.contains(&id) {
            Ok(())
        } else {
            Err(AppError::Invalid(format!("o bundle referencia um perfil ausente: {id}")))
        }
    };
    if let Some(s) = &b.squad {
        for m in &s.squad.members {
            check_profile(&m.profile_id)?;
        }
    }
    if let Some(r) = &b.recipe {
        for t in r.plan.iter().flatten() {
            if let Some(p) = &t.profile_id {
                check_profile(p)?;
            }
        }
        if let Some(sid) = &r.squad_id {
            let in_bundle = b.squad.as_ref().is_some_and(|s| &s.ref_id == sid);
            if !in_bundle && squads::get(conn, sid).is_err() {
                return Err(AppError::Invalid("a receita referencia um squad ausente".into()));
            }
        }
    }
    if !b.profiles.is_empty() {
        warnings.push("Instruções de agentes importados são texto de terceiros: leia-as antes de importar.".into());
    }
    Ok(Preview {
        kind: b.kind.clone(),
        name: b.name.clone(),
        profiles: b
            .profiles
            .iter()
            .map(|p| PreviewProfile { name: p.profile.name.clone(), provider_id: p.profile.provider_id.clone(), permission: p.profile.permission.clone(), instructions: p.profile.instructions.clone() })
            .collect(),
        squad: b.squad.as_ref().map(|s| format!("{} ({} membros)", s.squad.name, s.squad.members.len())),
        recipe: b.recipe.as_ref().map(|r| r.name.clone()),
        warnings,
        bundle: b,
    })
}

fn unique_name(existing: &[String], name: &str) -> String {
    if !existing.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        return name.to_string();
    }
    let base = format!("{name} (importado)");
    let mut candidate = base.clone();
    let mut i = 2;
    while existing.iter().any(|n| n.eq_ignore_ascii_case(&candidate)) {
        candidate = format!("{base} {i}");
        i += 1;
    }
    candidate
}

/// Imports a previewed bundle (re-validated here) in a single transaction.
pub fn import(conn: &Connection, bundle: Bundle) -> AppResult<ImportResult> {
    let bundle = parse(&serde_json::to_string(&bundle)?)?;
    let p = preview(conn, bundle)?;
    let b = p.bundle;
    let tx = conn.unchecked_transaction()?;
    let mut profile_map: HashMap<String, String> = HashMap::new();
    let mut names: Vec<String> = profiles::list(&tx)?.into_iter().map(|p| p.name).collect();
    for bp in &b.profiles {
        let mut input = bp.profile.clone();
        input.name = unique_name(&names, &input.name);
        names.push(input.name.clone());
        let created = profiles::create_with_origin(&tx, &input, "import")?;
        profile_map.insert(bp.ref_id.clone(), created.id);
    }
    let map_profile = |id: &str| profile_map.get(id).cloned().unwrap_or_else(|| id.to_string());
    let mut squad_id = None;
    if let Some(bs) = &b.squad {
        let mut input = bs.squad.clone();
        let existing: Vec<String> = squads::list(&tx)?.into_iter().map(|s| s.name).collect();
        input.name = unique_name(&existing, &input.name);
        for m in &mut input.members {
            m.profile_id = map_profile(&m.profile_id);
        }
        let created = squads::create(&tx, &input, "import")?;
        squad_id = Some((bs.ref_id.clone(), created.id));
    }
    let mut recipe_id = None;
    if let Some(r) = &b.recipe {
        let mut input = r.clone();
        let existing: Vec<String> = recipes::list(&tx)?.into_iter().map(|r| r.name).collect();
        input.name = unique_name(&existing, &input.name);
        if let (Some(sid), Some((ref_id, new_id))) = (&input.squad_id, &squad_id) {
            if sid == ref_id {
                input.squad_id = Some(new_id.clone());
            }
        }
        if let Some(plan) = &mut input.plan {
            for t in plan {
                t.profile_id = t.profile_id.as_deref().map(map_profile);
            }
        }
        recipe_id = Some(recipes::create(&tx, &input, "import")?.id);
    }
    tx.commit()?;
    Ok(ImportResult { profiles: profile_map.len(), squad_id: squad_id.map(|(_, id)| id), recipe_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            profiles::ensure_builtins(c, "claude")?;
            squads::ensure_builtins(c)?;
            recipes::ensure_builtins(c)
        })
        .unwrap();
        db
    }

    #[test]
    fn roundtrip_squad_with_custom_profile_and_safety() {
        let db = db();
        db.with(|c| {
            let mut pi = ProfileInput {
                name: "Rust Expert".into(),
                role: "rust".into(),
                description: "".into(),
                provider_id: "claude".into(),
                model: None,
                instructions: "Write idiomatic Rust.".into(),
                permission: "full".into(),
                max_turns: None,
            };
            let custom = profiles::create(c, &pi)?;
            pi.permission = "edit".into();
            let squad = squads::create(
                c,
                &squads::SquadInput {
                    name: "Rust Team".into(),
                    description: "".into(),
                    members: vec![
                        squads::Member { profile_id: custom.id.clone(), function: "Dev".into(), stage: 1 },
                        squads::Member { profile_id: "builtin:qa".into(), function: "QA".into(), stage: 2 },
                    ],
                    rules: "".into(),
                    flow: "".into(),
                    default_mode: "squad".into(),
                },
                "local",
            )?;
            let bundle = export_squad(c, &squad.id)?;
            assert_eq!(bundle.profiles.len(), 1, "only custom profiles are embedded");
            let text = serde_json::to_string_pretty(&bundle)?;

            let preview = preview(c, parse(&text)?)?;
            assert_eq!(preview.profiles[0].permission, "edit", "full is downgraded");
            assert!(preview.warnings.iter().any(|w| w.contains("rebaixado")));

            let res = import(c, preview.bundle)?;
            assert_eq!(res.profiles, 1);
            let imported = squads::get(c, res.squad_id.as_deref().unwrap())?;
            assert_eq!(imported.name, "Rust Team (importado)");
            assert_eq!(imported.origin, "import");
            assert_ne!(imported.members[0].profile_id, custom.id, "new profile ids");
            let p = profiles::get(c, &imported.members[0].profile_id)?;
            assert_eq!((p.origin.as_str(), p.permission.as_str(), p.name.as_str()), ("import", "edit", "Rust Expert (importado)"));
            assert_eq!(imported.members[1].profile_id, "builtin:qa");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn roundtrip_recipe_with_builtin_squad() {
        let db = db();
        db.with(|c| {
            let b = export_recipe(c, "builtin:bugfix")?;
            assert!(b.profiles.is_empty());
            let res = import(c, parse(&serde_json::to_string(&b)?)?)?;
            let r = recipes::get(c, res.recipe_id.as_deref().unwrap())?;
            assert_eq!(r.name, "Corrigir bugs (importado)");
            assert_eq!(r.plan.as_ref().map(Vec::len), Some(3));
            assert!(res.squad_id.is_some(), "squad copied as an imported squad");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rejects_malformed_or_dangerous_bundles() {
        let db = db();
        db.with(|c| {
            assert!(parse("{}").is_err());
            assert!(parse(&"x".repeat(MAX_BYTES + 1)).is_err());
            let mut b = export_recipe(c, "builtin:api")?;
            b.version = 2;
            assert!(parse(&serde_json::to_string(&b)?).is_err());
            let mut b = export_recipe(c, "builtin:api")?;
            b.kind = "virus".into();
            assert!(parse(&serde_json::to_string(&b)?).is_err());
            let text = serde_json::to_string(&export_recipe(c, "builtin:api")?)?.replacen("{", "{\"exec\":\"rm -rf /\",", 1);
            assert!(parse(&text).is_err(), "unknown fields rejected");
            let mut b = export_squad(c, "builtin:arcade")?;
            b.squad.as_mut().unwrap().squad.members[0].profile_id = "ghost".into();
            assert!(preview(c, b).is_err(), "dangling profile reference");
            let mut b = export_squad(c, "builtin:arcade")?;
            b.profiles.push(BundleProfile {
                ref_id: "x".into(),
                profile: ProfileInput { name: "X".into(), role: "x".into(), description: "".into(), provider_id: "evil-cli".into(), model: None, instructions: "".into(), permission: "read".into(), max_turns: None },
            });
            assert!(preview(c, b).is_err(), "unknown provider");
            Ok(())
        })
        .unwrap();
    }
}
