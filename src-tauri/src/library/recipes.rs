//! Recipes: mission templates (objective, squad, Maestro mode/limits, a
//! briefing template with typed fields and an optional preset plan).

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::squads;
use crate::agents::profiles;
use crate::db::{missions, new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::missions::tasks::{self, TaskInput};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub objective: String,
    pub squad_id: Option<String>,
    pub mode: String,
    pub max_parallel: i64,
    pub max_attempts: i64,
    pub max_runs: i64,
    pub budget_usd: Option<f64>,
    pub require_review: bool,
    pub briefing_template: String,
    pub fields: Vec<Field>,
    pub plan: Option<Vec<TaskInput>>,
    pub builtin: bool,
    pub origin: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecipeInput {
    pub name: String,
    #[serde(default)]
    pub objective: String,
    pub squad_id: Option<String>,
    pub mode: String,
    pub max_parallel: i64,
    pub max_attempts: i64,
    pub max_runs: i64,
    pub budget_usd: Option<f64>,
    pub require_review: bool,
    pub briefing_template: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    pub plan: Option<Vec<TaskInput>>,
}

const COLS: &str = "id, name, objective, squad_id, mode, max_parallel, max_attempts, max_runs, budget_usd, require_review,
  briefing_template, fields, plan, builtin, origin, created_at, updated_at";

fn map(r: &Row) -> rusqlite::Result<Recipe> {
    let fields: String = r.get(11)?;
    let plan: Option<String> = r.get(12)?;
    Ok(Recipe {
        id: r.get(0)?,
        name: r.get(1)?,
        objective: r.get(2)?,
        squad_id: r.get(3)?,
        mode: r.get(4)?,
        max_parallel: r.get(5)?,
        max_attempts: r.get(6)?,
        max_runs: r.get(7)?,
        budget_usd: r.get(8)?,
        require_review: r.get::<_, i64>(9)? != 0,
        briefing_template: r.get(10)?,
        fields: serde_json::from_str(&fields).unwrap_or_default(),
        plan: plan.and_then(|p| serde_json::from_str(&p).ok()),
        builtin: r.get::<_, i64>(13)? != 0,
        origin: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

// ── Placeholders ──────────────────────────────────────────────────────────

/// `{{key}}` placeholders used in a text.
pub fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find("{{") {
        let after = &rest[a + 2..];
        let Some(b) = after.find("}}") else { break };
        let key = after[..b].trim().to_string();
        if !key.is_empty() && !out.contains(&key) {
            out.push(key);
        }
        rest = &after[b + 2..];
    }
    out
}

pub fn render(text: &str, values: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v.trim());
    }
    out
}

fn f(key: &str, label: &str, placeholder: &str, required: bool) -> Field {
    Field { key: key.into(), label: label.into(), placeholder: placeholder.into(), required }
}

fn t(key: &str, title: &str, profile: &str, deps: &[&str], description: &str, acceptance: &str) -> TaskInput {
    TaskInput {
        key: key.into(),
        title: title.into(),
        description: description.into(),
        acceptance: acceptance.into(),
        profile_id: Some(profile.into()),
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
    }
}

struct BuiltinRecipe {
    id: &'static str,
    input: fn() -> RecipeInput,
}

fn base(name: &str, objective: &str, squad: Option<&str>, mode: &str, template: &str, fields: Vec<Field>, plan: Option<Vec<TaskInput>>) -> RecipeInput {
    RecipeInput {
        name: name.into(),
        objective: objective.into(),
        squad_id: squad.map(str::to_string),
        mode: mode.into(),
        max_parallel: 2,
        max_attempts: 3,
        max_runs: 40,
        budget_usd: None,
        require_review: true,
        briefing_template: template.into(),
        fields,
        plan,
    }
}

const BUILTINS: &[BuiltinRecipe] = &[
    BuiltinRecipe {
        id: "builtin:saas",
        input: || {
            base(
                "Criar SaaS completo",
                "Construir um SaaS com autenticação, dashboard, pagamentos e gestão de usuários.",
                Some("builtin:saas-factory"),
                "squad",
                "Desenvolva o SaaS \"{{produto}}\" para {{publico}}.\n\nFuncionalidades essenciais: autenticação, dashboard, pagamentos ({{pagamentos}}) e gerenciamento de usuários.\nStack preferida: {{stack}}.\n\nRequisitos adicionais: {{extras}}",
                vec![
                    f("produto", "Produto", "ex.: Agenda Pro", true),
                    f("publico", "Público-alvo", "ex.: clínicas pequenas", true),
                    f("pagamentos", "Pagamentos", "ex.: Stripe, assinatura mensal", true),
                    f("stack", "Stack", "ex.: a já usada no repositório", false),
                    f("extras", "Requisitos adicionais", "", false),
                ],
                None,
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:landing",
        input: || {
            base(
                "Criar landing page premium",
                "Landing page de alto impacto, rápida, acessível e persuasiva.",
                Some("builtin:cinema-site"),
                "squad",
                "Crie uma landing page premium para {{produto}}.\n\nObjetivo da página: {{objetivo}}.\nPúblico: {{publico}}.\nTom e estilo: {{tom}}.\nSeções desejadas: {{secoes}}.\n\nA página deve ser responsiva, acessível (WCAG AA) e rápida.",
                vec![
                    f("produto", "Produto", "", true),
                    f("objetivo", "Objetivo (conversão)", "ex.: captar leads para o beta", true),
                    f("publico", "Público", "", true),
                    f("tom", "Tom e estilo", "ex.: cinematográfico, minimalista, dark", false),
                    f("secoes", "Seções", "ex.: hero, benefícios, prova social, preços, FAQ, CTA", false),
                ],
                None,
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:api",
        input: || {
            base(
                "Desenvolver API",
                "API documentada, validada e testada.",
                Some("builtin:app-factory"),
                "squad",
                "Desenvolva uma API para {{dominio}}.\n\nRecursos/endpoints: {{recursos}}.\nAutenticação: {{auth}}.\nPersistência: {{banco}}.\n\nInclua validação de entrada, tratamento de erros consistente, documentação (OpenAPI ou equivalente) e testes automatizados.",
                vec![
                    f("dominio", "Domínio", "ex.: gestão de pedidos", true),
                    f("recursos", "Recursos", "ex.: clientes, pedidos, produtos", true),
                    f("auth", "Autenticação", "ex.: JWT", false),
                    f("banco", "Banco de dados", "ex.: PostgreSQL", false),
                ],
                None,
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:refactor",
        input: || {
            base(
                "Refatorar projeto",
                "Melhorar estrutura e manutenibilidade sem mudar comportamento.",
                Some("builtin:autopilot"),
                "squad",
                "Refatore {{alvo}}.\n\nObjetivo da refatoração: {{objetivo}}.\nRestrições: o comportamento externo não pode mudar; os testes existentes devem continuar passando.",
                vec![f("alvo", "O que refatorar", "ex.: módulo de pagamentos (src/billing)", true), f("objetivo", "Objetivo", "ex.: separar regras de negócio da camada HTTP", true)],
                Some(vec![
                    t("map", "Mapear o código atual", "builtin:researcher", &[], "Map the current structure of {{alvo}}: modules, responsibilities, dependencies, test coverage and risky areas. Report with file references.", "A report listing modules, dependencies and existing tests for {{alvo}}."),
                    t("plan", "Plano de refatoração", "builtin:architect", &["map"], "Based on the mapping, design an incremental refactoring plan for {{alvo}} to achieve: {{objetivo}}. Keep external behavior unchanged.", "A step-by-step plan with target structure and risk mitigation."),
                    t("do", "Executar a refatoração", "builtin:executor", &["plan"], "Execute the refactoring of {{alvo}} following the plan ({{objetivo}}). Keep behavior unchanged and run the existing tests.", "Code restructured; all existing tests pass; no behavior change."),
                    t("review", "Revisar a refatoração", "builtin:reviewer", &["do"], "Review the refactoring of {{alvo}} for behavior changes, regressions and clarity.", "Review report with no blocking issues."),
                ]),
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:bugfix",
        input: || {
            base(
                "Corrigir bugs",
                "Reproduzir, diagnosticar, corrigir e prevenir regressão.",
                Some("builtin:autopilot"),
                "agentic",
                "Corrija o seguinte problema: {{bug}}\n\nComo reproduzir: {{reproducao}}\nComportamento esperado: {{esperado}}",
                vec![
                    f("bug", "Descrição do bug", "", true),
                    f("reproducao", "Como reproduzir", "passos, entrada, ambiente", false),
                    f("esperado", "Comportamento esperado", "", true),
                ],
                Some(vec![
                    t("diagnose", "Reproduzir e diagnosticar", "builtin:researcher", &[], "Reproduce and diagnose this bug: {{bug}}. Reproduction: {{reproducao}}. Expected: {{esperado}}. Find the root cause with evidence (files, lines).", "Root cause identified with file/line references."),
                    t("fix", "Corrigir com teste de regressão", "builtin:executor", &["diagnose"], "Fix the root cause of: {{bug}} (expected: {{esperado}}). Add a regression test that fails before the fix and passes after.", "Bug fixed; regression test added and passing; existing tests pass."),
                    t("verify", "Verificar a correção", "builtin:qa", &["fix"], "Verify the fix for: {{bug}}. Run the test suite and try edge cases around the change.", "Tests pass; no regressions found in related areas."),
                ]),
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:security-audit",
        input: || {
            let mut r = base(
                "Auditar segurança",
                "Auditoria de segurança em somente leitura, com achados priorizados.",
                None,
                "free",
                "Audite a segurança de {{escopo}}.\n\nFoco: {{foco}}.\nContexto de ameaça: {{contexto}}",
                vec![
                    f("escopo", "Escopo", "ex.: todo o repositório, ou src/api", true),
                    f("foco", "Foco", "ex.: autenticação, injeção, segredos, dependências", false),
                    f("contexto", "Contexto", "ex.: app exposto na internet com dados pessoais", false),
                ],
                Some(vec![
                    t("audit", "Auditoria de segurança", "builtin:security", &[], "Perform a security audit of {{escopo}} focusing on {{foco}}. Threat context: {{contexto}}. For each finding: severity, location, exploit scenario and fix. Never print secret values.", "Prioritized findings with locations and concrete fixes."),
                    t("deps", "Dependências e configuração", "builtin:devops", &[], "Review dependency manifests, lockfiles, CI and deployment configuration of {{escopo}} for known-vulnerable or risky settings. Report only; do not change files.", "List of risky dependencies/configuration with recommended actions."),
                ]),
            );
            r.max_parallel = 2;
            r
        },
    },
    BuiltinRecipe {
        id: "builtin:mobile",
        input: || {
            base(
                "Criar aplicativo mobile",
                "App mobile com navegação, telas principais e integração com dados.",
                Some("builtin:app-factory"),
                "squad",
                "Crie um aplicativo mobile \"{{app}}\" para {{publico}}.\n\nTelas principais: {{telas}}.\nTecnologia: {{tecnologia}}.\nIntegrações: {{integracoes}}",
                vec![
                    f("app", "Nome do app", "", true),
                    f("publico", "Público", "", true),
                    f("telas", "Telas principais", "ex.: login, feed, perfil, configurações", true),
                    f("tecnologia", "Tecnologia", "ex.: React Native + Expo", false),
                    f("integracoes", "Integrações", "ex.: API REST existente, push notifications", false),
                ],
                None,
            )
        },
    },
    BuiltinRecipe {
        id: "builtin:db-migration",
        input: || {
            base(
                "Migrar banco de dados",
                "Migração segura de esquema ou de banco, com plano de rollback.",
                Some("builtin:app-factory"),
                "squad",
                "Planeje e implemente a migração: {{migracao}}.\n\nBanco atual: {{origem}}. Destino: {{destino}}.\nRestrições: {{restricoes}}\n\nA migração deve preservar os dados, ser reversível sempre que possível e incluir testes.",
                vec![
                    f("migracao", "Migração", "ex.: separar tabela users em users + profiles", true),
                    f("origem", "Origem", "ex.: PostgreSQL 14", false),
                    f("destino", "Destino", "ex.: mesmo banco, novo esquema", false),
                    f("restricoes", "Restrições", "ex.: sem downtime", false),
                ],
                None,
            )
        },
    },
];

pub fn ensure_builtins(conn: &Connection) -> AppResult<()> {
    for b in BUILTINS {
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM recipes WHERE id = ?1)", [b.id], |r| r.get(0))?;
        if !exists {
            insert(conn, b.id, &(b.input)(), true, "builtin")?;
        }
    }
    Ok(())
}

pub fn restore_builtin(conn: &Connection, id: &str) -> AppResult<Recipe> {
    let b = BUILTINS.iter().find(|b| b.id == id).ok_or_else(|| AppError::Invalid("não é uma receita padrão".into()))?;
    write(conn, id, &(b.input)())?;
    get(conn, id)
}

pub fn list(conn: &Connection) -> AppResult<Vec<Recipe>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM recipes ORDER BY builtin DESC, name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Recipe> {
    conn.query_row(&format!("SELECT {COLS} FROM recipes WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("receita não encontrada: {id}")))
}

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 40 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn validate(conn: &Connection, input: &RecipeInput) -> AppResult<()> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(AppError::Invalid("o nome da receita deve ter de 1 a 120 caracteres".into()));
    }
    if !["free", "squad", "agentic"].contains(&input.mode.as_str()) {
        return Err(AppError::Invalid("modo inválido".into()));
    }
    if !(1..=4).contains(&input.max_parallel) || !(1..=10).contains(&input.max_attempts) || !(1..=500).contains(&input.max_runs) {
        return Err(AppError::Invalid("limites inválidos".into()));
    }
    if input.budget_usd.is_some_and(|b| !(b > 0.0 && b < 100_000.0)) {
        return Err(AppError::Invalid("orçamento inválido".into()));
    }
    if input.briefing_template.trim().is_empty() || input.briefing_template.chars().count() > 20_000 || input.objective.chars().count() > 1000 {
        return Err(AppError::Invalid("o modelo de briefing é obrigatório (até 20.000 caracteres)".into()));
    }
    if let Some(s) = &input.squad_id {
        squads::get(conn, s)?;
    }
    if input.fields.len() > 20 {
        return Err(AppError::Invalid("no máximo 20 campos".into()));
    }
    let mut keys = std::collections::HashSet::new();
    for fl in &input.fields {
        if !valid_key(&fl.key) || !keys.insert(fl.key.as_str()) {
            return Err(AppError::Invalid(format!("campo inválido ou duplicado: {:?}", fl.key)));
        }
        if fl.label.trim().is_empty() || fl.label.chars().count() > 80 || fl.placeholder.chars().count() > 200 {
            return Err(AppError::Invalid(format!("rótulo inválido no campo {}", fl.key)));
        }
    }
    let mut texts = vec![input.briefing_template.clone()];
    if let Some(plan) = &input.plan {
        let all = profiles::list(conn)?;
        tasks::validate(plan, &all)?;
        for t in plan {
            texts.extend([t.title.clone(), t.description.clone(), t.acceptance.clone()]);
        }
    }
    for text in texts {
        for p in placeholders(&text) {
            if !keys.contains(p.as_str()) {
                return Err(AppError::Invalid(format!("o placeholder {{{{{p}}}}} não corresponde a nenhum campo")));
            }
        }
    }
    Ok(())
}

fn insert(conn: &Connection, id: &str, input: &RecipeInput, builtin: bool, origin: &str) -> AppResult<()> {
    let now = now_ms();
    conn.execute(
        &format!("INSERT INTO recipes ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16)"),
        params![
            id,
            input.name.trim(),
            input.objective.trim(),
            input.squad_id,
            input.mode,
            input.max_parallel,
            input.max_attempts,
            input.max_runs,
            input.budget_usd,
            input.require_review as i64,
            input.briefing_template,
            serde_json::to_string(&input.fields)?,
            input.plan.as_ref().map(serde_json::to_string).transpose()?,
            builtin as i64,
            origin,
            now
        ],
    )?;
    Ok(())
}

fn write(conn: &Connection, id: &str, input: &RecipeInput) -> AppResult<()> {
    conn.execute(
        "UPDATE recipes SET name = ?2, objective = ?3, squad_id = ?4, mode = ?5, max_parallel = ?6, max_attempts = ?7, max_runs = ?8,
         budget_usd = ?9, require_review = ?10, briefing_template = ?11, fields = ?12, plan = ?13, updated_at = ?14 WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.objective.trim(),
            input.squad_id,
            input.mode,
            input.max_parallel,
            input.max_attempts,
            input.max_runs,
            input.budget_usd,
            input.require_review as i64,
            input.briefing_template,
            serde_json::to_string(&input.fields)?,
            input.plan.as_ref().map(serde_json::to_string).transpose()?,
            now_ms()
        ],
    )?;
    Ok(())
}

pub fn create(conn: &Connection, input: &RecipeInput, origin: &str) -> AppResult<Recipe> {
    validate(conn, input)?;
    let id = new_id();
    insert(conn, &id, input, false, origin)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: &RecipeInput) -> AppResult<Recipe> {
    validate(conn, input)?;
    get(conn, id)?;
    write(conn, id, input)?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if get(conn, id)?.builtin {
        return Err(AppError::Invalid("receitas padrão não podem ser excluídas (use Restaurar padrão)".into()));
    }
    conn.execute("DELETE FROM recipes WHERE id = ?1", [id])?;
    Ok(())
}

pub fn to_input(r: &Recipe) -> RecipeInput {
    RecipeInput {
        name: r.name.clone(),
        objective: r.objective.clone(),
        squad_id: r.squad_id.clone(),
        mode: r.mode.clone(),
        max_parallel: r.max_parallel,
        max_attempts: r.max_attempts,
        max_runs: r.max_runs,
        budget_usd: r.budget_usd,
        require_review: r.require_review,
        briefing_template: r.briefing_template.clone(),
        fields: r.fields.clone(),
        plan: r.plan.clone(),
    }
}

/// Creates a mission from a recipe: renders the briefing (and preset plan) with
/// the given values and applies squad, mode and limits.
pub fn instantiate(conn: &Connection, recipe_id: &str, workspace_id: Option<&str>, title: &str, values: &HashMap<String, String>) -> AppResult<missions::Mission> {
    let r = get(conn, recipe_id)?;
    for fl in &r.fields {
        let v = values.get(&fl.key).map(|v| v.trim()).unwrap_or_default();
        if fl.required && v.is_empty() {
            return Err(AppError::Invalid(format!("preencha o campo “{}”", fl.label)));
        }
        if v.chars().count() > 5_000 {
            return Err(AppError::Invalid(format!("o campo “{}” é longo demais", fl.label)));
        }
    }
    // Optional fields left empty render as "não especificado".
    let mut filled: HashMap<String, String> = HashMap::new();
    for fl in &r.fields {
        let v = values.get(&fl.key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| "não especificado".into());
        filled.insert(fl.key.clone(), v);
    }
    let briefing = render(&r.briefing_template, &filled);
    let tx = conn.unchecked_transaction()?;
    let m = missions::create(&tx, &missions::NewMission { workspace_id: workspace_id.map(str::to_string), title: title.to_string(), briefing, mode: r.mode.clone() })?;
    tx.execute("UPDATE missions SET squad_id = ?2, recipe_id = ?3 WHERE id = ?1", params![m.id, r.squad_id, r.id])?;
    missions::set_maestro(
        &tx,
        &m.id,
        &missions::MaestroSettings { mode: r.mode.clone(), max_parallel: r.max_parallel, require_review: r.require_review, max_attempts: r.max_attempts, max_runs: r.max_runs, budget_usd: r.budget_usd },
    )?;
    if let Some(plan) = &r.plan {
        let rendered: Vec<TaskInput> = plan
            .iter()
            .map(|t| TaskInput { title: render(&t.title, &filled), description: render(&t.description, &filled), acceptance: render(&t.acceptance, &filled), ..t.clone() })
            .collect();
        let all = profiles::list(&tx)?;
        let created = tasks::replace(&tx, &m.id, &rendered, &all)?;
        missions::set_status(&tx, &m.id, "planned")?;
        tasks::event(&tx, &m.id, None, "plan", &format!("Plano da receita “{}” aplicado ({} tarefas) — revise e aprove para iniciar", r.name, created.len()))?;
    } else {
        tasks::event(&tx, &m.id, None, "plan", &format!("Missão criada a partir da receita “{}”", r.name))?;
    }
    tx.commit()?;
    missions::get(conn, &m.id)
}

/// Saves a mission (briefing, squad, Maestro settings and current plan) as a reusable recipe.
pub fn from_mission(conn: &Connection, mission_id: &str, name: &str) -> AppResult<Recipe> {
    let m = missions::get(conn, mission_id)?;
    let squad_id: Option<String> = conn.query_row("SELECT squad_id FROM missions WHERE id = ?1", [mission_id], |r| r.get(0))?;
    let task_list = tasks::list(conn, mission_id)?;
    let key_of: HashMap<String, String> = task_list.iter().map(|t| (t.id.clone(), t.key.clone())).collect();
    // Braces from mission text must not be mistaken for placeholders.
    let esc = |s: &str| s.replace("{{", "{ {").replace("}}", "} }");
    let plan: Vec<TaskInput> = task_list
        .iter()
        .map(|t| TaskInput {
            key: t.key.clone(),
            title: esc(&t.title),
            description: esc(&t.description),
            acceptance: esc(&t.acceptance),
            profile_id: t.profile_id.clone(),
            depends_on: t.depends_on.iter().filter_map(|d| key_of.get(d).cloned()).collect(),
        })
        .collect();
    let template = esc(&m.briefing);
    let input = RecipeInput {
        name: name.to_string(),
        objective: m.title.clone(),
        squad_id,
        mode: m.mode.clone(),
        max_parallel: m.max_parallel,
        max_attempts: m.max_attempts,
        max_runs: m.max_runs,
        budget_usd: m.budget_usd,
        require_review: m.require_review,
        briefing_template: template,
        fields: vec![],
        plan: (!plan.is_empty()).then_some(plan),
    };
    create(conn, &input, "local")
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
            ensure_builtins(c)
        })
        .unwrap();
        db
    }

    #[test]
    fn seeds_eight_valid_recipes() {
        let db = db();
        db.with(|c| {
            ensure_builtins(c)?;
            let all = list(c)?;
            assert_eq!(all.len(), 8);
            for r in &all {
                validate(c, &to_input(r))?; // placeholders declared, plans valid, squads exist
            }
            assert!(all.iter().filter(|r| r.plan.is_some()).count() >= 3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn placeholders_and_render() {
        assert_eq!(placeholders("a {{x}} b {{ y }} {{x}}"), ["x", "y"]);
        let v = HashMap::from([("x".to_string(), " 1 ".to_string())]);
        assert_eq!(render("v={{x}}", &v), "v=1");
    }

    #[test]
    fn validation_rejects_undeclared_placeholders_and_bad_limits() {
        let db = db();
        db.with(|c| {
            let mut r = to_input(&get(c, "builtin:api")?);
            r.briefing_template.push_str(" {{nope}}");
            assert!(validate(c, &r).unwrap_err().to_string().contains("nope"));
            let mut r = to_input(&get(c, "builtin:api")?);
            r.max_parallel = 9;
            assert!(validate(c, &r).is_err());
            let mut r = to_input(&get(c, "builtin:api")?);
            r.squad_id = Some("ghost".into());
            assert!(validate(c, &r).is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn instantiate_with_and_without_preset_plan() {
        let db = db();
        db.with(|c| {
            let ws = crate::db::workspaces::add(c, "/tmp/forge-recipe-ws", "ws")?;
            assert!(instantiate(c, "builtin:bugfix", Some(&ws.id), "Fix", &HashMap::new()).is_err(), "required fields");
            let values = HashMap::from([("bug".to_string(), "login falha com e-mail em maiúsculas".to_string()), ("esperado".to_string(), "login funciona".to_string())]);
            let m = instantiate(c, "builtin:bugfix", Some(&ws.id), "Fix login", &values)?;
            assert_eq!((m.status.as_str(), m.mode.as_str()), ("planned", "agentic"));
            assert!(m.briefing.contains("maiúsculas") && m.briefing.contains("não especificado"));
            let t = tasks::list(c, &m.id)?;
            assert_eq!(t.len(), 3);
            assert!(t[0].description.contains("maiúsculas") && !t[0].description.contains("{{"));
            let squad: Option<String> = c.query_row("SELECT squad_id FROM missions WHERE id = ?1", [&m.id], |r| r.get(0))?;
            assert_eq!(squad.as_deref(), Some("builtin:autopilot"));

            let vals = HashMap::from([("dominio".to_string(), "pedidos".to_string()), ("recursos".to_string(), "clientes".to_string())]);
            let api = instantiate(c, "builtin:api", Some(&ws.id), "API", &vals)?;
            assert_eq!(api.status, "draft");
            assert!(tasks::list(c, &api.id)?.is_empty());

            // Round trip: save a mission as a recipe.
            let saved = from_mission(c, &m.id, "Minha correção")?;
            assert_eq!(saved.plan.as_ref().map(Vec::len), Some(3));
            assert_eq!(saved.plan.as_ref().unwrap()[1].depends_on, vec!["diagnose".to_string()]);
            assert_eq!(saved.squad_id.as_deref(), Some("builtin:autopilot"));
            Ok(())
        })
        .unwrap();
    }
}
