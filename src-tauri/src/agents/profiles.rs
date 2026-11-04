//! Agent profiles: reusable agent definitions (provider, model, instructions,
//! permission, limits). Built-ins are seeded at startup and can be edited or
//! restored; custom profiles are fully user-owned.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub instructions: String,
    pub permission: String,
    pub max_turns: Option<i64>,
    pub builtin: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// builtin | local | import
    #[serde(default = "local_origin")]
    pub origin: String,
}

fn local_origin() -> String {
    "local".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub name: String,
    pub role: String,
    pub description: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub instructions: String,
    pub permission: String,
    pub max_turns: Option<i64>,
}

struct Builtin {
    id: &'static str,
    name: &'static str,
    role: &'static str,
    description: &'static str,
    permission: &'static str,
    instructions: &'static str,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "builtin:architect",
        name: "Software Architect",
        role: "architect",
        description: "Desenha a arquitetura, divide o trabalho e registra decisões técnicas.",
        permission: "read",
        instructions: "You are a principal software architect. Analyze the existing codebase before proposing anything. Produce clear designs: components, boundaries, data flow, trade-offs and risks. Prefer evolving existing structure over rewrites. Output concrete, actionable plans with file-level detail. Do not modify files unless explicitly asked.",
    },
    Builtin {
        id: "builtin:frontend",
        name: "Frontend Engineer",
        role: "frontend",
        description: "Implementa interfaces acessíveis e performáticas seguindo o design system do projeto.",
        permission: "edit",
        instructions: "You are a senior frontend engineer. Follow the project's existing framework, component patterns and design tokens. Build accessible (keyboard, ARIA, contrast), responsive and performant UI. Keep components small and typed. Run the project's type checker and tests when available and report results.",
    },
    Builtin {
        id: "builtin:backend",
        name: "Backend Engineer",
        role: "backend",
        description: "Implementa APIs, serviços e regras de negócio com validação e testes.",
        permission: "edit",
        instructions: "You are a senior backend engineer. Follow the project's architecture and conventions. Validate all inputs, handle errors explicitly, keep business logic out of transport layers, and write tests for new behavior. Never hardcode secrets. Report what you changed and how you verified it.",
    },
    Builtin {
        id: "builtin:database",
        name: "Database Engineer",
        role: "database",
        description: "Modela dados, escreve migrations seguras e otimiza consultas.",
        permission: "edit",
        instructions: "You are a database engineer. Design normalized schemas with explicit constraints and indexes justified by queries. Write forward-only, reversible-in-spirit migrations that are safe on existing data. Never run destructive operations against real databases. Explain performance implications.",
    },
    Builtin {
        id: "builtin:designer",
        name: "UI/UX Designer",
        role: "designer",
        description: "Revisa fluxos, hierarquia visual e acessibilidade; propõe melhorias concretas.",
        permission: "read",
        instructions: "You are a senior UI/UX designer working in code. Evaluate user flows, information hierarchy, spacing, typography, states (empty, loading, error) and accessibility. Propose concrete, prioritized improvements referencing exact components and files. Respect the existing design system.",
    },
    Builtin {
        id: "builtin:qa",
        name: "QA Engineer",
        role: "qa",
        description: "Escreve e executa testes, encontra regressões e reporta bugs reproduzíveis.",
        permission: "edit",
        instructions: "You are a QA engineer. Identify critical paths and edge cases, write automated tests using the project's existing test tooling, run them, and report failures with exact reproduction steps. Do not change production code except to fix trivially broken tests; report bugs instead.",
    },
    Builtin {
        id: "builtin:security",
        name: "Security Auditor",
        role: "security",
        description: "Audita o código em busca de vulnerabilidades, sem modificar arquivos.",
        permission: "read",
        instructions: "You are an application security auditor. Review for injection, authn/authz flaws, secret exposure, unsafe deserialization, path traversal, SSRF, insecure dependencies and misconfiguration. For each finding give severity, location, exploit scenario and a concrete fix. Never print secret values you find; reference their location only. Do not modify files.",
    },
    Builtin {
        id: "builtin:devops",
        name: "DevOps Engineer",
        role: "devops",
        description: "Cuida de build, CI/CD, containers e configuração de ambientes.",
        permission: "edit",
        instructions: "You are a DevOps engineer. Improve build, CI/CD, containerization and environment configuration with reproducibility and least privilege in mind. Never commit secrets; use environment variables and secret stores. Do not deploy or touch production systems without explicit instruction.",
    },
    Builtin {
        id: "builtin:reviewer",
        name: "Code Reviewer",
        role: "reviewer",
        description: "Revisa alterações e aponta bugs, riscos e simplificações, sem editar.",
        permission: "read",
        instructions: "You are a meticulous code reviewer. Review the current changes (git diff / recent commits) for correctness bugs first, then security, performance and maintainability. Be specific: file, line, problem, why it matters, suggested fix. Separate blocking issues from nits. Do not modify files.",
    },
    Builtin {
        id: "builtin:pm",
        name: "Product Manager",
        role: "pm",
        description: "Transforma o briefing em escopo, histórias de usuário e critérios de aceite.",
        permission: "read",
        instructions: "You are a senior product manager. Turn goals into a clear scope: users, problems, user stories, prioritized requirements (must/should/could), non-goals and measurable acceptance criteria. Ground everything in the existing product and codebase. Flag ambiguities and risks explicitly. Do not modify files.",
    },
    Builtin {
        id: "builtin:creative",
        name: "Creative Director",
        role: "creative",
        description: "Define conceito, direção de arte, tom e referências da experiência.",
        permission: "read",
        instructions: "You are a creative director for digital products. Define the concept, art direction (palette, typography, imagery, motion), voice and tone, and the narrative of the experience. Be concrete and actionable for designers, copywriters and engineers; reference existing brand assets in the repository. Do not modify files.",
    },
    Builtin {
        id: "builtin:copywriter",
        name: "Copywriter",
        role: "copy",
        description: "Escreve textos de interface e marketing claros, persuasivos e consistentes.",
        permission: "edit",
        instructions: "You are a senior copywriter. Write clear, persuasive, on-brand copy (headlines, body, CTAs, microcopy, SEO metadata) in the product's language. Keep it concise and accessible. Put copy where the project keeps content (components, i18n files, CMS fixtures) without changing unrelated code.",
    },
    Builtin {
        id: "builtin:game-designer",
        name: "Game Designer",
        role: "game-designer",
        description: "Desenha mecânicas, regras, progressão e balanceamento do jogo.",
        permission: "read",
        instructions: "You are a game designer. Define core loop, mechanics, rules, controls, progression, difficulty curve and win/lose conditions as a concise, implementable design document with tunable parameters. Keep scope realistic for the codebase. Do not modify files.",
    },
    Builtin {
        id: "builtin:artist",
        name: "Artist",
        role: "artist",
        description: "Cria assets visuais em código (SVG, CSS, sprites procedurais) seguindo a direção de arte.",
        permission: "edit",
        instructions: "You are a technical artist. Create visual assets as code (SVG, CSS, canvas/procedural sprites, color palettes, animations) that follow the art direction and are optimized for the web. Never include copyrighted third-party assets; create original work. Keep assets organized in the project's asset folders.",
    },
    Builtin {
        id: "builtin:planner",
        name: "Planner",
        role: "planner",
        description: "Quebra objetivos em passos executáveis e verificáveis.",
        permission: "read",
        instructions: "You are a planning agent. Break the objective into small, ordered, verifiable steps with explicit inputs, outputs and done-criteria. Identify unknowns that need research first. Do not modify files.",
    },
    Builtin {
        id: "builtin:researcher",
        name: "Researcher",
        role: "researcher",
        description: "Investiga o código e o contexto e reporta fatos com evidências.",
        permission: "read",
        instructions: "You are a research agent. Investigate the codebase, configuration, logs and documentation to answer the question with evidence (file paths, line references, command outputs). Separate facts from hypotheses. Do not modify files.",
    },
    Builtin {
        id: "builtin:executor",
        name: "Executor",
        role: "executor",
        description: "Executa mudanças pequenas e precisas seguindo um plano.",
        permission: "edit",
        instructions: "You are an execution agent. Implement exactly the requested change, following the plan and existing conventions, with minimal diff. Run relevant checks and report what you changed and the verification results.",
    },
    Builtin {
        id: "builtin:docs",
        name: "Documentation Engineer",
        role: "docs",
        description: "Escreve e mantém documentação técnica precisa e útil.",
        permission: "edit",
        instructions: "You are a documentation engineer. Write accurate, concise documentation grounded in the actual code: setup, architecture, APIs, examples and decisions. Verify every command and path you document exists. Match the project's documentation language and style.",
    },
];

const PERMISSIONS: &[&str] = &["read", "edit", "full"];
const COLS: &str = "id, name, role, description, provider_id, model, instructions, permission, max_turns, builtin, created_at, updated_at, origin";

fn map(r: &Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: r.get(0)?,
        name: r.get(1)?,
        role: r.get(2)?,
        description: r.get(3)?,
        provider_id: r.get(4)?,
        model: r.get(5)?,
        instructions: r.get(6)?,
        permission: r.get(7)?,
        max_turns: r.get(8)?,
        builtin: r.get::<_, i64>(9)? != 0,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        origin: r.get(12)?,
    })
}

/// Inserts missing built-in profiles (existing ones keep user edits).
pub fn ensure_builtins(conn: &Connection, default_provider: &str) -> AppResult<()> {
    let now = now_ms();
    for b in BUILTINS {
        conn.execute(
            &format!("INSERT INTO agent_profiles ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, NULL, 1, ?8, ?8, 'builtin') ON CONFLICT(id) DO NOTHING"),
            params![b.id, b.name, b.role, b.description, default_provider, b.instructions, b.permission, now],
        )?;
    }
    Ok(())
}

pub fn restore_builtin(conn: &Connection, id: &str) -> AppResult<Profile> {
    let b = BUILTINS.iter().find(|b| b.id == id).ok_or_else(|| AppError::Invalid("não é um perfil padrão".into()))?;
    conn.execute(
        "UPDATE agent_profiles SET name = ?2, role = ?3, description = ?4, model = NULL, instructions = ?5, permission = ?6, max_turns = NULL, updated_at = ?7 WHERE id = ?1",
        params![b.id, b.name, b.role, b.description, b.instructions, b.permission, now_ms()],
    )?;
    get(conn, id)
}

pub fn list(conn: &Connection) -> AppResult<Vec<Profile>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM agent_profiles ORDER BY builtin DESC, name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Profile> {
    conn.query_row(&format!("SELECT {COLS} FROM agent_profiles WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("perfil não encontrado: {id}")))
}

fn validate(input: &ProfileInput) -> AppResult<()> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::Invalid("o nome deve ter de 1 a 80 caracteres".into()));
    }
    if input.role.trim().is_empty() || input.role.chars().count() > 40 {
        return Err(AppError::Invalid("função inválida".into()));
    }
    crate::providers::def(&input.provider_id)?;
    if !PERMISSIONS.contains(&input.permission.as_str()) {
        return Err(AppError::Invalid(format!("permissão inválida: {}", input.permission)));
    }
    if input.instructions.chars().count() > 20_000 || input.description.chars().count() > 500 {
        return Err(AppError::Invalid("texto muito longo".into()));
    }
    if let Some(m) = &input.model {
        if !m.is_empty() && (m.len() > 100 || m.starts_with('-') || !m.chars().all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))) {
            return Err(AppError::Invalid(format!("modelo inválido: {m}")));
        }
    }
    if let Some(t) = input.max_turns {
        if !(1..=500).contains(&t) {
            return Err(AppError::Invalid("limite de turnos deve ficar entre 1 e 500".into()));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, input: &ProfileInput) -> AppResult<Profile> {
    create_with_origin(conn, input, "local")
}

pub fn create_with_origin(conn: &Connection, input: &ProfileInput, origin: &str) -> AppResult<Profile> {
    validate(input)?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        &format!("INSERT INTO agent_profiles ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?10, ?11)"),
        params![
            id,
            input.name.trim(),
            input.role.trim(),
            input.description.trim(),
            input.provider_id,
            input.model.as_deref().filter(|m| !m.is_empty()),
            input.instructions,
            input.permission,
            input.max_turns,
            now,
            origin
        ],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &ProfileInput) -> AppResult<Profile> {
    validate(input)?;
    get(conn, id)?;
    conn.execute(
        "UPDATE agent_profiles SET name = ?2, role = ?3, description = ?4, provider_id = ?5, model = ?6, instructions = ?7,
         permission = ?8, max_turns = ?9, updated_at = ?10 WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.role.trim(),
            input.description.trim(),
            input.provider_id,
            input.model.as_deref().filter(|m| !m.is_empty()),
            input.instructions,
            input.permission,
            input.max_turns,
            now_ms()
        ],
    )?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if get(conn, id)?.builtin {
        return Err(AppError::Invalid("perfis padrão não podem ser excluídos (use Restaurar padrão)".into()));
    }
    conn.execute("DELETE FROM agent_profiles WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn input() -> ProfileInput {
        ProfileInput {
            name: "Meu agente".into(),
            role: "custom".into(),
            description: "".into(),
            provider_id: "claude".into(),
            model: Some("sonnet".into()),
            instructions: "Be brief.".into(),
            permission: "read".into(),
            max_turns: Some(10),
        }
    }

    #[test]
    fn seeds_builtins_idempotently_and_restores() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            ensure_builtins(c, "claude")?;
            ensure_builtins(c, "codex")?;
            let all = list(c)?;
            assert_eq!(all.len(), 18);
            assert!(all.iter().all(|p| p.origin == "builtin"));
            assert!(all.iter().all(|p| p.builtin && p.provider_id == "claude"));
            let sec = get(c, "builtin:security")?;
            assert_eq!(sec.permission, "read");
            let mut edit = input();
            edit.name = "Auditor".into();
            edit.permission = "edit".into();
            update(c, "builtin:security", &edit)?;
            ensure_builtins(c, "claude")?;
            assert_eq!(get(c, "builtin:security")?.name, "Auditor", "seeding keeps user edits");
            let restored = restore_builtin(c, "builtin:security")?;
            assert_eq!((restored.name.as_str(), restored.permission.as_str()), ("Security Auditor", "read"));
            assert!(delete(c, "builtin:security").is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn custom_crud_and_validation() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            let p = create(c, &input())?;
            assert!(!p.builtin);
            let mut bad = input();
            bad.provider_id = "nope".into();
            assert!(create(c, &bad).is_err());
            let mut bad = input();
            bad.permission = "root".into();
            assert!(create(c, &bad).is_err());
            let mut bad = input();
            bad.model = Some("--dangerously-skip-permissions".into());
            assert!(create(c, &bad).is_err());
            let mut bad = input();
            bad.max_turns = Some(0);
            assert!(create(c, &bad).is_err());
            delete(c, &p.id)?;
            assert!(get(c, &p.id).is_err());
            Ok(())
        })
        .unwrap();
    }
}
