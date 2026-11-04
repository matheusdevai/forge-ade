//! Squads: named teams of agent profiles with functions, stages (flow order)
//! and rules that the Maestro applies when planning and executing missions.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::agents::profiles::{self, Profile};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

pub const MAX_MEMBERS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub profile_id: String,
    /// Function inside this squad (e.g. "Developer").
    pub function: String,
    /// Flow order: lower stages work first (1..=9).
    pub stage: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Squad {
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<Member>,
    pub rules: String,
    pub flow: String,
    pub default_mode: String,
    pub builtin: bool,
    pub origin: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SquadInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub members: Vec<Member>,
    #[serde(default)]
    pub rules: String,
    #[serde(default)]
    pub flow: String,
    pub default_mode: String,
}

const COLS: &str = "id, name, description, members, rules, flow, default_mode, builtin, origin, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Squad> {
    let members: String = r.get(3)?;
    Ok(Squad {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        members: serde_json::from_str(&members).unwrap_or_default(),
        rules: r.get(4)?,
        flow: r.get(5)?,
        default_mode: r.get(6)?,
        builtin: r.get::<_, i64>(7)? != 0,
        origin: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

struct BuiltinSquad {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    members: &'static [(&'static str, &'static str, i64)],
    rules: &'static str,
    flow: &'static str,
    mode: &'static str,
}

const BUILTINS: &[BuiltinSquad] = &[
    BuiltinSquad {
        id: "builtin:app-factory",
        name: "APP FACTORY",
        description: "Equipe completa para construir aplicações: arquitetura, backend, frontend, dados e qualidade.",
        members: &[
            ("builtin:architect", "Architect", 1),
            ("builtin:database", "Database", 2),
            ("builtin:backend", "Backend", 2),
            ("builtin:frontend", "Frontend", 3),
            ("builtin:qa", "QA", 4),
        ],
        rules: "Start with an architecture task that fixes contracts (APIs, data model) before implementation. Backend and frontend tasks must follow those contracts. Every feature ends with tests. Keep tasks small enough for one agent.",
        flow: "Architect → Database/Backend → Frontend → QA",
        mode: "squad",
    },
    BuiltinSquad {
        id: "builtin:cinema-site",
        name: "CINEMA SITE",
        description: "Sites de alto impacto visual: conceito criativo, design, texto, implementação e QA.",
        members: &[
            ("builtin:creative", "Creative Director", 1),
            ("builtin:designer", "Designer", 2),
            ("builtin:copywriter", "Copywriter", 2),
            ("builtin:frontend", "Frontend", 3),
            ("builtin:qa", "QA", 4),
        ],
        rules: "The creative direction task defines concept, palette, typography and tone first. Design and copy follow it. Frontend implements with attention to performance (Core Web Vitals), accessibility and responsive behavior. Use only original assets.",
        flow: "Creative Director → Designer + Copywriter → Frontend → QA",
        mode: "squad",
    },
    BuiltinSquad {
        id: "builtin:saas-factory",
        name: "SAAS FACTORY",
        description: "Produtos SaaS: escopo de produto, arquitetura, backend, frontend, segurança e QA.",
        members: &[
            ("builtin:pm", "Product Manager", 1),
            ("builtin:architect", "Architect", 1),
            ("builtin:backend", "Backend", 2),
            ("builtin:frontend", "Frontend", 3),
            ("builtin:security", "Security", 4),
            ("builtin:qa", "QA", 4),
        ],
        rules: "Begin with product scope and architecture. Authentication, authorization, multi-tenancy and billing boundaries must be explicit. Security reviews auth, data access and secrets handling before release. Never hardcode credentials; use environment variables.",
        flow: "PM + Architect → Backend → Frontend → Security + QA",
        mode: "squad",
    },
    BuiltinSquad {
        id: "builtin:autopilot",
        name: "AUTOPILOT",
        description: "Ciclo enxuto planejar → pesquisar → executar → revisar, ideal para tarefas pontuais.",
        members: &[
            ("builtin:planner", "Planner", 1),
            ("builtin:researcher", "Researcher", 2),
            ("builtin:executor", "Executor", 3),
            ("builtin:reviewer", "Reviewer", 4),
        ],
        rules: "Prefer few, sequential tasks. Research before changing code. The executor makes minimal, focused changes. The reviewer verifies against the acceptance criteria.",
        flow: "Planner → Researcher → Executor → Reviewer",
        mode: "agentic",
    },
    BuiltinSquad {
        id: "builtin:arcade",
        name: "ARCADE",
        description: "Jogos: game design, desenvolvimento, arte em código e QA.",
        members: &[
            ("builtin:game-designer", "Game Designer", 1),
            ("builtin:frontend", "Developer", 2),
            ("builtin:artist", "Artist", 2),
            ("builtin:qa", "QA", 3),
        ],
        rules: "The game design task defines mechanics and tunable parameters first. Development implements a playable core loop early, then iterates. Art is original and created as code/assets in the repo. QA plays through edge cases (input, collisions, win/lose).",
        flow: "Game Designer → Developer + Artist → QA",
        mode: "squad",
    },
];

pub fn ensure_builtins(conn: &Connection) -> AppResult<()> {
    let now = now_ms();
    for b in BUILTINS {
        let members: Vec<Member> = b.members.iter().map(|(p, f, s)| Member { profile_id: p.to_string(), function: f.to_string(), stage: *s }).collect();
        conn.execute(
            &format!("INSERT INTO squads ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 'builtin', ?8, ?8) ON CONFLICT(id) DO NOTHING"),
            params![b.id, b.name, b.description, serde_json::to_string(&members)?, b.rules, b.flow, b.mode, now],
        )?;
    }
    Ok(())
}

pub fn restore_builtin(conn: &Connection, id: &str) -> AppResult<Squad> {
    let b = BUILTINS.iter().find(|b| b.id == id).ok_or_else(|| AppError::Invalid("não é um squad padrão".into()))?;
    let members: Vec<Member> = b.members.iter().map(|(p, f, s)| Member { profile_id: p.to_string(), function: f.to_string(), stage: *s }).collect();
    conn.execute(
        "UPDATE squads SET name = ?2, description = ?3, members = ?4, rules = ?5, flow = ?6, default_mode = ?7, updated_at = ?8 WHERE id = ?1",
        params![b.id, b.name, b.description, serde_json::to_string(&members)?, b.rules, b.flow, b.mode, now_ms()],
    )?;
    get(conn, id)
}

pub fn list(conn: &Connection) -> AppResult<Vec<Squad>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM squads ORDER BY builtin DESC, name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Squad> {
    conn.query_row(&format!("SELECT {COLS} FROM squads WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("squad não encontrado: {id}")))
}

pub fn validate(conn: &Connection, input: &SquadInput) -> AppResult<()> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::Invalid("o nome do squad deve ter de 1 a 80 caracteres".into()));
    }
    if input.members.is_empty() || input.members.len() > MAX_MEMBERS {
        return Err(AppError::Invalid(format!("um squad precisa de 1 a {MAX_MEMBERS} membros")));
    }
    if !["free", "squad", "agentic"].contains(&input.default_mode.as_str()) {
        return Err(AppError::Invalid("modo padrão inválido".into()));
    }
    if input.rules.chars().count() > 10_000 || input.flow.chars().count() > 500 || input.description.chars().count() > 500 {
        return Err(AppError::Invalid("texto muito longo".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for m in &input.members {
        profiles::get(conn, &m.profile_id)?;
        if !(1..=9).contains(&m.stage) {
            return Err(AppError::Invalid("a etapa de cada membro deve ficar entre 1 e 9".into()));
        }
        if m.function.trim().is_empty() || m.function.chars().count() > 60 {
            return Err(AppError::Invalid("cada membro precisa de uma função (até 60 caracteres)".into()));
        }
        if !seen.insert(m.profile_id.as_str()) {
            return Err(AppError::Invalid("o mesmo perfil aparece duas vezes no squad".into()));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, input: &SquadInput, origin: &str) -> AppResult<Squad> {
    validate(conn, input)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        &format!("INSERT INTO squads ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?9)"),
        params![id, input.name.trim(), input.description.trim(), serde_json::to_string(&input.members)?, input.rules, input.flow.trim(), input.default_mode, origin, now],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &SquadInput) -> AppResult<Squad> {
    validate(conn, input)?;
    get(conn, id)?;
    conn.execute(
        "UPDATE squads SET name = ?2, description = ?3, members = ?4, rules = ?5, flow = ?6, default_mode = ?7, updated_at = ?8 WHERE id = ?1",
        params![id, input.name.trim(), input.description.trim(), serde_json::to_string(&input.members)?, input.rules, input.flow.trim(), input.default_mode, now_ms()],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if get(conn, id)?.builtin {
        return Err(AppError::Invalid("squads padrão não podem ser excluídos (use Restaurar padrão)".into()));
    }
    conn.execute("DELETE FROM squads WHERE id = ?1", [id])?;
    Ok(())
}

/// Profiles of a squad, in member order (missing profiles are skipped).
pub fn member_profiles(conn: &Connection, squad: &Squad) -> AppResult<Vec<Profile>> {
    let mut out = Vec::new();
    for m in &squad.members {
        if let Ok(p) = profiles::get(conn, &m.profile_id) {
            out.push(p);
        }
    }
    Ok(out)
}

/// Planner guidance derived from the squad (functions, stages, rules).
pub fn planning_context(squad: &Squad, members: &[Profile]) -> String {
    let mut s = format!("This mission is executed by the squad \"{}\". Assign tasks ONLY to its members.\n", squad.name);
    if !squad.flow.trim().is_empty() {
        s.push_str(&format!("Squad flow: {}\n", squad.flow.trim()));
    }
    s.push_str("Members (stage = order in the flow; earlier stages produce inputs for later ones):\n");
    for m in &squad.members {
        if let Some(p) = members.iter().find(|p| p.id == m.profile_id) {
            s.push_str(&format!("- role `{}` — {} (stage {})\n", p.role, m.function, m.stage));
        }
    }
    if !squad.rules.trim().is_empty() {
        s.push_str(&format!("Squad rules:\n{}\n", squad.rules.trim()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            profiles::ensure_builtins(c, "claude")?;
            ensure_builtins(c)
        })
        .unwrap();
        db
    }

    #[test]
    fn seeds_the_five_spec_squads_with_valid_members() {
        let db = db();
        db.with(|c| {
            ensure_builtins(c)?;
            let all = list(c)?;
            let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
            for n in ["APP FACTORY", "CINEMA SITE", "SAAS FACTORY", "AUTOPILOT", "ARCADE"] {
                assert!(names.contains(&n), "missing {n}");
            }
            for s in &all {
                let input = SquadInput { name: s.name.clone(), description: s.description.clone(), members: s.members.clone(), rules: s.rules.clone(), flow: s.flow.clone(), default_mode: s.default_mode.clone() };
                validate(c, &input)?; // every builtin member points to a real profile
                assert_eq!(member_profiles(c, s)?.len(), s.members.len());
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn crud_validation_and_restore() {
        let db = db();
        db.with(|c| {
            let input = SquadInput {
                name: "Mini".into(),
                description: "".into(),
                members: vec![Member { profile_id: "builtin:backend".into(), function: "Dev".into(), stage: 1 }],
                rules: "".into(),
                flow: "".into(),
                default_mode: "free".into(),
            };
            let s = create(c, &input, "local")?;
            let mut bad = input.clone();
            bad.members.push(bad.members[0].clone());
            assert!(create(c, &bad, "local").is_err(), "duplicate member");
            let mut bad = input.clone();
            bad.members[0].profile_id = "ghost".into();
            assert!(create(c, &bad, "local").is_err());
            let mut bad = input.clone();
            bad.members[0].stage = 0;
            assert!(create(c, &bad, "local").is_err());
            delete(c, &s.id)?;
            let mut edit = input.clone();
            edit.name = "Changed".into();
            update(c, "builtin:arcade", &edit)?;
            assert_eq!(restore_builtin(c, "builtin:arcade")?.name, "ARCADE");
            assert!(delete(c, "builtin:arcade").is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn planning_context_lists_members_and_rules() {
        let db = db();
        db.with(|c| {
            let s = get(c, "builtin:cinema-site")?;
            let ctx = planning_context(&s, &member_profiles(c, &s)?);
            assert!(ctx.contains("CINEMA SITE") && ctx.contains("role `copy`") && ctx.contains("stage 3") && ctx.contains("original assets"));
            Ok(())
        })
        .unwrap();
    }
}
