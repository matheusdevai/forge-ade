//! Skills persistence: CRUD with version history, supporting files, links to
//! profiles/squads/missions, and composition of the instructions injected
//! into agent runs.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::format::{self, ScannedSkill, SkillDoc, SkillFile};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};

/// Max characters of skill text injected into a single run.
pub const MAX_INJECTED: usize = 48 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub body: String,
    pub enabled: bool,
    pub builtin: bool,
    pub origin: String,
    pub source: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersion {
    pub version: i64,
    pub name: String,
    pub description: String,
    pub body: String,
    pub note: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub target_kind: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill: Skill,
    pub versions: Vec<SkillVersion>,
    pub files: Vec<SkillFile>,
    pub links: Vec<Link>,
}

const COLS: &str = "id, slug, name, description, version, body, enabled, builtin, origin, source, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Skill> {
    Ok(Skill {
        id: r.get(0)?,
        slug: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        version: r.get(4)?,
        body: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        builtin: r.get::<_, i64>(7)? != 0,
        origin: r.get(8)?,
        source: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

// ── Built-ins ─────────────────────────────────────────────────────────────

struct Builtin {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    body: &'static str,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "builtin:react-expert",
        name: "React Expert",
        description: "Boas práticas de React moderno: componentes, hooks, estado, acessibilidade e performance.",
        body: "# React Expert\n\n- Prefer function components and hooks; keep components small and focused.\n- Derive state instead of duplicating it; lift state only as far as needed; colocate state with its usage.\n- `useEffect` only for synchronizing with external systems — not for derived data or event handling.\n- Stable keys for lists (never array indexes when items can reorder).\n- Memoize (`useMemo`/`useCallback`/`memo`) only when profiling or referential stability requires it.\n- Accessibility: semantic HTML first, labelled controls, keyboard support, visible focus, ARIA only when needed.\n- Type props precisely (TypeScript); avoid `any`.\n- Handle loading, empty and error states explicitly.\n- Follow the project's existing styling and state-management conventions before introducing new libraries.",
    },
    Builtin {
        id: "builtin:nextjs-expert",
        name: "Next.js Expert",
        description: "Next.js App Router: Server Components, Server Actions, roteamento, cache e deploy.",
        body: "# Next.js Expert\n\n- Check the installed Next.js version and router (App Router vs Pages) before writing code; follow it.\n- App Router: Server Components by default; add `\"use client\"` only for interactivity/browser APIs, as low in the tree as possible.\n- Fetch data on the server; use Server Actions for mutations with input validation and authorization checks.\n- Be explicit about caching and revalidation; never cache user-specific data globally.\n- Use `next/image`, `next/font`, metadata APIs and route handlers idiomatically.\n- Keep secrets server-only (never in client components or `NEXT_PUBLIC_*`).\n- Handle `loading.tsx`, `error.tsx` and `not-found.tsx` for routes you add.",
    },
    Builtin {
        id: "builtin:supabase-expert",
        name: "Supabase Expert",
        description: "Supabase: Postgres, Row Level Security, Auth, migrations e uso seguro das chaves.",
        body: "# Supabase Expert\n\n- Every table exposed to clients must have Row Level Security enabled with explicit policies per operation.\n- Write schema changes as versioned SQL migrations; never edit applied migrations.\n- The service-role key is server-only; clients use the anon/publishable key.\n- Use Auth helpers for the framework in use; validate the session server-side before privileged operations.\n- Index columns used in RLS policies and frequent filters.\n- Prefer database constraints (FK, CHECK, UNIQUE) over application-only validation.\n- Generate and use TypeScript types from the schema when the project does.",
    },
    Builtin {
        id: "builtin:saas-architect",
        name: "SaaS Architect",
        description: "Arquitetura de SaaS: multi-tenancy, autenticação, billing, limites e observabilidade.",
        body: "# SaaS Architect\n\n- Make tenancy explicit: every tenant-owned row carries the tenant id; enforce isolation at the data layer.\n- Separate authentication (who) from authorization (what): roles/permissions checked server-side on every mutation.\n- Billing: subscription state comes from the payment provider via verified webhooks (idempotent handlers); never trust the client.\n- Plan limits/quotas enforced server-side; design for upgrades/downgrades and trials.\n- Audit log for security-relevant actions; structured logs and error tracking from day one.\n- Configuration and secrets via environment; no secrets in the repo.\n- Document key decisions (ADR) and contracts between modules.",
    },
    Builtin {
        id: "builtin:security-auditor",
        name: "Security Auditor",
        description: "Checklist de auditoria de segurança de aplicações (OWASP) com achados acionáveis.",
        body: "# Security Auditor\n\nReview against this checklist and report findings as: severity · location · scenario · fix.\n\n- Injection (SQL/NoSQL/command/template), XSS, SSRF, path traversal, unsafe deserialization.\n- AuthN/AuthZ: missing checks, IDOR, privilege escalation, session handling, CSRF.\n- Secrets: hardcoded keys, secrets in logs, client bundles or repo history (report location only — never print values).\n- Dependencies: known-vulnerable or abandoned packages; lockfile integrity.\n- Data protection: PII exposure, encryption in transit/at rest, overly broad CORS.\n- Configuration: debug modes, default credentials, permissive cloud/IAM settings.\n- Prioritize exploitable issues over theoretical ones.",
    },
    Builtin {
        id: "builtin:ui-designer",
        name: "UI Designer",
        description: "Design de interface em código: hierarquia, espaçamento, tipografia, estados e acessibilidade.",
        body: "# UI Designer\n\n- Respect the existing design system (tokens, components, spacing scale); extend it instead of inventing one-offs.\n- Clear visual hierarchy: one primary action per view; consistent alignment and spacing rhythm.\n- Typography: limited sizes/weights, comfortable line-height, readable line length.\n- Design every state: empty, loading, error, success, disabled, long content.\n- Accessibility: contrast ≥ WCAG AA, focus states, hit targets ≥ 24px, no information by color alone, reduced motion.\n- Responsive by default; test narrow and wide layouts.\n- Microcopy: concise, specific, action-oriented.",
    },
    Builtin {
        id: "builtin:testing-expert",
        name: "Testing Expert",
        description: "Estratégia e escrita de testes automatizados confiáveis e rápidos.",
        body: "# Testing Expert\n\n- Use the project's existing test framework and conventions; run the suite before and after changes.\n- Test behavior through public interfaces, not implementation details.\n- Each test: clear arrange/act/assert, one reason to fail, descriptive name.\n- Cover edge cases and error paths, not only the happy path; add a regression test for every bug fix.\n- Avoid flakiness: no real time/network/randomness without control; await conditions instead of sleeping.\n- Keep unit tests fast; use integration tests for boundaries (DB, HTTP) with realistic fixtures.\n- Report exact commands run and results.",
    },
];

pub fn ensure_builtins(conn: &Connection) -> AppResult<()> {
    let now = now_ms();
    for b in BUILTINS {
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM skills WHERE id = ?1)", [b.id], |r| r.get(0))?;
        if exists {
            continue;
        }
        let slug = format::slug(b.name);
        conn.execute(
            &format!("INSERT INTO skills ({COLS}) VALUES (?1, ?2, ?3, ?4, 1, ?5, 1, 1, 'builtin', NULL, ?6, ?6)"),
            params![b.id, slug, b.name, b.description, b.body, now],
        )?;
        conn.execute(
            "INSERT INTO skill_versions (skill_id, version, name, description, body, note, created_at) VALUES (?1, 1, ?2, ?3, ?4, 'versão padrão', ?5)",
            params![b.id, b.name, b.description, b.body, now],
        )?;
    }
    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────────────

pub fn list(conn: &Connection) -> AppResult<Vec<Skill>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM skills ORDER BY builtin DESC, name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Skill> {
    conn.query_row(&format!("SELECT {COLS} FROM skills WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("skill não encontrada: {id}")))
}

fn by_slug(conn: &Connection, slug: &str) -> AppResult<Option<Skill>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM skills WHERE slug = ?1"), [slug], map).optional()?)
}

pub fn detail(conn: &Connection, id: &str) -> AppResult<SkillDetail> {
    let skill = get(conn, id)?;
    let mut stmt = conn.prepare("SELECT version, name, description, body, note, created_at FROM skill_versions WHERE skill_id = ?1 ORDER BY version DESC")?;
    let versions = stmt
        .query_map([id], |r| Ok(SkillVersion { version: r.get(0)?, name: r.get(1)?, description: r.get(2)?, body: r.get(3)?, note: r.get(4)?, created_at: r.get(5)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare("SELECT path, size, is_script, content FROM skill_files WHERE skill_id = ?1 ORDER BY path")?;
    let files = stmt
        .query_map([id], |r| Ok(SkillFile { path: r.get(0)?, size: r.get::<_, i64>(1)? as u64, is_script: r.get::<_, i64>(2)? != 0, content: r.get(3)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare("SELECT target_kind, target_id FROM skill_links WHERE skill_id = ?1 ORDER BY target_kind, target_id")?;
    let links = stmt.query_map([id], |r| Ok(Link { target_kind: r.get(0)?, target_id: r.get(1)? }))?.collect::<Result<Vec<_>, _>>()?;
    Ok(SkillDetail { skill, versions, files, links })
}

// ── Mutations ─────────────────────────────────────────────────────────────

fn add_version(conn: &Connection, id: &str, version: i64, doc: &SkillDoc, note: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO skill_versions (skill_id, version, name, description, body, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, version, doc.name, doc.description, doc.body, note, now_ms()],
    )?;
    Ok(())
}

fn replace_files(conn: &Connection, id: &str, files: &[SkillFile]) -> AppResult<()> {
    conn.execute("DELETE FROM skill_files WHERE skill_id = ?1", [id])?;
    for f in files {
        conn.execute(
            "INSERT INTO skill_files (skill_id, path, size, is_script, content) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, f.path, f.size as i64, f.is_script as i64, f.content],
        )?;
    }
    Ok(())
}

pub fn create(conn: &Connection, doc: &SkillDoc, origin: &str, source: Option<&str>, files: &[SkillFile]) -> AppResult<Skill> {
    format::validate(doc)?;
    let slug = format::slug(&doc.name);
    if by_slug(conn, &slug)?.is_some() {
        return Err(AppError::Invalid(format!("já existe uma skill “{}”", doc.name)));
    }
    let id = new_id();
    let now = now_ms();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("INSERT INTO skills ({COLS}) VALUES (?1, ?2, ?3, ?4, 1, ?5, 1, 0, ?6, ?7, ?8, ?8)"),
        params![id, slug, doc.name, doc.description, doc.body, origin, source, now],
    )?;
    add_version(&tx, &id, 1, doc, if origin == "import" { "instalada" } else { "criada" })?;
    replace_files(&tx, &id, files)?;
    tx.commit()?;
    get(conn, &id)
}

/// Saves new content as a new version (no-op if nothing changed).
pub fn update(conn: &Connection, id: &str, doc: &SkillDoc, note: &str) -> AppResult<Skill> {
    format::validate(doc)?;
    let current = get(conn, id)?;
    if current.name == doc.name && current.description == doc.description && current.body == doc.body {
        return Ok(current);
    }
    let slug = format::slug(&doc.name);
    if let Some(other) = by_slug(conn, &slug)? {
        if other.id != id {
            return Err(AppError::Invalid(format!("já existe outra skill “{}”", doc.name)));
        }
    }
    let version = current.version + 1;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE skills SET slug = ?2, name = ?3, description = ?4, body = ?5, version = ?6, updated_at = ?7 WHERE id = ?1",
        params![id, slug, doc.name, doc.description, doc.body, version, now_ms()],
    )?;
    add_version(&tx, id, version, doc, note)?;
    tx.commit()?;
    get(conn, id)
}

/// Restores an old version by creating a new version with its content.
pub fn restore_version(conn: &Connection, id: &str, version: i64) -> AppResult<Skill> {
    let v = detail(conn, id)?.versions.into_iter().find(|v| v.version == version).ok_or_else(|| AppError::NotFound(format!("versão {version} não encontrada")))?;
    update(conn, id, &SkillDoc { name: v.name, description: v.description, body: v.body }, &format!("restaurada da v{version}"))
}

pub fn restore_builtin(conn: &Connection, id: &str) -> AppResult<Skill> {
    let b = BUILTINS.iter().find(|b| b.id == id).ok_or_else(|| AppError::Invalid("não é uma skill padrão".into()))?;
    update(conn, id, &SkillDoc { name: b.name.into(), description: b.description.into(), body: b.body.into() }, "padrão restaurado")
}

/// Installs a scanned skill. If a skill with the same name exists and
/// `update_existing` is set, its content becomes a new version.
pub fn install(conn: &Connection, scanned: &ScannedSkill, update_existing: bool) -> AppResult<Skill> {
    let slug = format::slug(&scanned.doc.name);
    match by_slug(conn, &slug)? {
        Some(existing) if update_existing => {
            if existing.builtin {
                return Err(AppError::Invalid("não é possível sobrescrever uma skill padrão por instalação".into()));
            }
            let s = update(conn, &existing.id, &scanned.doc, "atualizada por instalação")?;
            replace_files(conn, &s.id, &scanned.files)?;
            conn.execute("UPDATE skills SET source = ?2, origin = 'import' WHERE id = ?1", params![s.id, scanned.source])?;
            get(conn, &s.id)
        }
        Some(_) => Err(AppError::Invalid(format!("já existe uma skill “{}” — escolha atualizar", scanned.doc.name))),
        None => create(conn, &scanned.doc, "import", Some(&scanned.source), &scanned.files),
    }
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> AppResult<Skill> {
    get(conn, id)?;
    conn.execute("UPDATE skills SET enabled = ?2, updated_at = ?3 WHERE id = ?1", params![id, enabled as i64, now_ms()])?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if get(conn, id)?.builtin {
        return Err(AppError::Invalid("skills padrão não podem ser removidas (desative-as)".into()));
    }
    conn.execute("DELETE FROM skills WHERE id = ?1", [id])?;
    Ok(())
}

fn check_target(conn: &Connection, kind: &str, target: &str) -> AppResult<()> {
    let table = match kind {
        "profile" => "agent_profiles",
        "squad" => "squads",
        "mission" => "missions",
        _ => return Err(AppError::Invalid(format!("tipo de associação inválido: {kind}"))),
    };
    let ok: bool = conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)"), [target], |r| r.get(0))?;
    if ok {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("{kind} não encontrado: {target}")))
    }
}

pub fn set_link(conn: &Connection, skill_id: &str, kind: &str, target: &str, linked: bool) -> AppResult<()> {
    get(conn, skill_id)?;
    check_target(conn, kind, target)?;
    if linked {
        conn.execute(
            "INSERT OR IGNORE INTO skill_links (skill_id, target_kind, target_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![skill_id, kind, target, now_ms()],
        )?;
    } else {
        conn.execute("DELETE FROM skill_links WHERE skill_id = ?1 AND target_kind = ?2 AND target_id = ?3", params![skill_id, kind, target])?;
    }
    Ok(())
}

/// Skills linked to a target (enabled or not).
pub fn linked_to(conn: &Connection, kind: &str, target: &str) -> AppResult<Vec<Skill>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM skills s JOIN skill_links l ON l.skill_id = s.id WHERE l.target_kind = ?1 AND l.target_id = ?2 ORDER BY s.name COLLATE NOCASE",
        COLS.split(", ").map(|c| format!("s.{c}")).collect::<Vec<_>>().join(", ")
    ))?;
    let rows = stmt.query_map(params![kind, target], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Composition ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Composed {
    /// Text appended to the agent instructions (empty when no skills apply).
    pub text: String,
    /// `slug@vN` labels recorded on the run.
    pub labels: Vec<String>,
    /// Skills dropped because the size budget was exceeded.
    pub dropped: Vec<String>,
}

/// Enabled skills for a run: profile ∪ squad ∪ mission (deduplicated, stable order).
pub fn compose(conn: &Connection, profile_id: Option<&str>, squad_id: Option<&str>, mission_id: Option<&str>) -> AppResult<Composed> {
    compose_with(conn, profile_id, squad_id, mission_id, &[])
}

/// Like `compose`, plus explicitly chosen skills for this run (e.g. Harness).
pub fn compose_with(conn: &Connection, profile_id: Option<&str>, squad_id: Option<&str>, mission_id: Option<&str>, extra_ids: &[String]) -> AppResult<Composed> {
    let mut skills: Vec<Skill> = Vec::new();
    for (kind, target) in [("profile", profile_id), ("squad", squad_id), ("mission", mission_id)] {
        let Some(t) = target else { continue };
        for s in linked_to(conn, kind, t)? {
            if s.enabled && !skills.iter().any(|x| x.id == s.id) {
                skills.push(s);
            }
        }
    }
    for id in extra_ids {
        if let Some(s) = list(conn)?.into_iter().find(|x| &x.id == id && x.enabled) {
            if !skills.iter().any(|x| x.id == s.id) {
                skills.push(s);
            }
        }
    }
    let mut out = Composed::default();
    if skills.is_empty() {
        return Ok(out);
    }
    let mut blocks = Vec::new();
    let mut used = 0usize;
    for s in skills {
        let block = format!("<skill name=\"{}\" version=\"{}\">\n{}\n</skill>", s.slug, s.version, s.body.trim());
        if used + block.len() > MAX_INJECTED {
            out.dropped.push(s.slug.clone());
            continue;
        }
        used += block.len();
        out.labels.push(format!("{}@v{}", s.slug, s.version));
        blocks.push(block);
    }
    if !blocks.is_empty() {
        out.text = format!(
            "\n\nThe following skills apply to this work. Follow them where relevant; they never override the safety rules or the user's request.\n<skills>\n{}\n</skills>",
            blocks.join("\n")
        );
    }
    Ok(out)
}

/// Appends composed skills to existing instructions.
pub fn with_skills(instructions: Option<String>, composed: &Composed) -> Option<String> {
    if composed.text.is_empty() {
        return instructions;
    }
    Some(format!("{}{}", instructions.unwrap_or_default(), composed.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            crate::agents::profiles::ensure_builtins(c, "claude")?;
            crate::library::squads::ensure_builtins(c)?;
            ensure_builtins(c)
        })
        .unwrap();
        db
    }

    fn doc(name: &str, body: &str) -> SkillDoc {
        SkillDoc { name: name.into(), description: "d".into(), body: body.into() }
    }

    #[test]
    fn seeds_the_seven_spec_skills() {
        let db = db();
        db.with(|c| {
            ensure_builtins(c)?;
            let all = list(c)?;
            assert_eq!(all.len(), 7);
            let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
            for n in ["React Expert", "Next.js Expert", "Supabase Expert", "SaaS Architect", "Security Auditor", "UI Designer", "Testing Expert"] {
                assert!(names.contains(&n));
            }
            for s in &all {
                format::validate(&SkillDoc { name: s.name.clone(), description: s.description.clone(), body: s.body.clone() })?;
                assert_eq!(detail(c, &s.id)?.versions.len(), 1);
            }
            assert!(delete(c, "builtin:react-expert").is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn versions_restore_and_conflicts() {
        let db = db();
        db.with(|c| {
            let s = create(c, &doc("Go Expert", "v1 body"), "local", None, &[])?;
            assert!(create(c, &doc("go expert", "x"), "local", None, &[]).is_err(), "slug conflict");
            let s2 = update(c, &s.id, &doc("Go Expert", "v2 body"), "edit")?;
            assert_eq!(s2.version, 2);
            assert_eq!(update(c, &s.id, &doc("Go Expert", "v2 body"), "noop")?.version, 2, "no-op save");
            let s3 = restore_version(c, &s.id, 1)?;
            assert_eq!((s3.version, s3.body.as_str()), (3, "v1 body"));
            assert_eq!(detail(c, &s.id)?.versions[0].note, "restaurada da v1");
            let edited = update(c, "builtin:ui-designer", &doc("UI Designer", "custom"), "edit")?;
            assert_eq!(edited.version, 2);
            assert_eq!(restore_builtin(c, "builtin:ui-designer")?.version, 3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn install_new_and_update_existing() {
        let db = db();
        db.with(|c| {
            let scanned = ScannedSkill {
                doc: doc("Rust Expert", "be idiomatic"),
                files: vec![SkillFile { path: "scripts/x.sh".into(), size: 3, is_script: true, content: Some("rm".into()) }],
                warnings: vec![],
                source: "/tmp/rust-expert".into(),
            };
            let s = install(c, &scanned, false)?;
            assert_eq!(s.origin, "import");
            assert_eq!(detail(c, &s.id)?.files.len(), 1);
            assert!(install(c, &scanned, false).is_err());
            let mut v2 = scanned.clone();
            v2.doc.body = "be very idiomatic".into();
            v2.files.clear();
            let s2 = install(c, &v2, true)?;
            assert_eq!((s2.id.as_str(), s2.version), (s.id.as_str(), 2));
            assert!(detail(c, &s.id)?.files.is_empty());
            let mut builtin = scanned.clone();
            builtin.doc.name = "React Expert".into();
            assert!(install(c, &builtin, true).is_err(), "builtins are not overwritten by installs");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn links_and_composition() {
        let db = db();
        db.with(|c| {
            let m = crate::db::missions::create(c, &crate::db::missions::NewMission { workspace_id: None, title: "m".into(), briefing: "b".into(), mode: "free".into() })?;
            set_link(c, "builtin:react-expert", "profile", "builtin:frontend", true)?;
            set_link(c, "builtin:testing-expert", "squad", "builtin:app-factory", true)?;
            set_link(c, "builtin:react-expert", "mission", &m.id, true)?; // duplicate via mission
            set_link(c, "builtin:security-auditor", "mission", &m.id, true)?;
            assert!(set_link(c, "builtin:react-expert", "profile", "ghost", true).is_err());
            assert!(set_link(c, "builtin:react-expert", "galaxy", "x", true).is_err());

            let comp = compose(c, Some("builtin:frontend"), Some("builtin:app-factory"), Some(&m.id))?;
            assert_eq!(comp.labels, ["react-expert@v1", "testing-expert@v1", "security-auditor@v1"]);
            assert!(comp.text.contains("<skill name=\"react-expert\"") && comp.text.matches("<skill name=\"react-expert\"").count() == 1);

            set_enabled(c, "builtin:security-auditor", false)?;
            assert_eq!(compose(c, None, None, Some(&m.id))?.labels, ["react-expert@v1"], "disabled skills are skipped");
            assert_eq!(compose(c, Some("builtin:qa"), None, None)?, Composed::default());
            assert_eq!(with_skills(Some("base".into()), &Composed::default()).as_deref(), Some("base"));

            // Budget: a huge skill is dropped, not truncated.
            let big = create(c, &doc("Huge", &"x".repeat(format::MAX_BODY - 10)), "local", None, &[])?;
            let big2 = create(c, &doc("Huge Two", &"y".repeat(format::MAX_BODY - 10)), "local", None, &[])?;
            set_link(c, &big.id, "profile", "builtin:qa", true)?;
            set_link(c, &big2.id, "profile", "builtin:qa", true)?;
            let comp = compose(c, Some("builtin:qa"), None, None)?;
            assert_eq!(comp.labels.len() + comp.dropped.len(), 2);
            assert!(comp.text.len() <= MAX_INJECTED + 400);
            set_link(c, "builtin:react-expert", "profile", "builtin:frontend", false)?;
            assert!(linked_to(c, "profile", "builtin:frontend")?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn extra_skills_are_added_once_and_only_when_enabled() {
        let db = db();
        db.with(|c| {
            let a = create(c, &doc("Harness Pick", "Use it."), "local", None, &[])?;
            let off = create(c, &doc("Disabled One", "Nope."), "local", None, &[])?;
            set_enabled(c, &off.id, false)?;
            let base = compose(c, Some("builtin:frontend"), None, None)?;
            let with = compose_with(c, Some("builtin:frontend"), None, None, &[a.id.clone(), a.id.clone(), off.id.clone(), "missing".into()])?;
            assert_eq!(with.labels.len(), base.labels.len() + 1);
            assert!(with.text.contains("harness-pick"));
            assert!(!with.text.contains("Nope."));
            Ok(())
        })
        .unwrap();
    }
}
