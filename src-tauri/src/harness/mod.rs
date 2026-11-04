//! Harness (M16): picks a configuration for a task — profile, provider,
//! model tier, effort (turn limit), permission, skills, MCP and worktree —
//! and explains every choice.
//!
//! Deterministic and honest:
//! - only providers that are installed and not logged out are considered;
//! - only real model aliases are proposed (Claude: haiku/sonnet/opus); other
//!   providers keep their CLI default;
//! - usage data comes from this machine's run history; costs only when the
//!   CLI reported them;
//! - permission is the least needed, never "full".

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

// ── Classification ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    /// question | review | plan | docs | tests | bugfix | refactor | feature | design
    pub activity: String,
    /// frontend | backend | database | devops | security | None
    pub domain: Option<String>,
    /// small | medium | large
    pub complexity: String,
    pub writes: bool,
    /// Keywords that drove the decision (shown to the user).
    pub signals: Vec<String>,
}

pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        let m = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(m);
    }
    format!(" {} ", out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Keyword lists match on word prefixes (" teste" matches "testes").
const ACTIVITIES: &[(&str, &[&str])] = &[
    ("review", &[" revis", " review", " auditar", " audite", " auditoria", " code review", " avalie o codigo"]),
    ("plan", &[" planej", " plano ", " arquitetura", " architect", " roadmap", " estrategia", " proposta", " design doc", " desenhe a solucao"]),
    ("docs", &[" documenta", " docs ", " readme", " documente", " changelog", " comentarios"]),
    ("tests", &[" teste", " test ", " tests", " testing", " spec ", " cobertura", " coverage", " vitest", " jest", " pytest", " e2e"]),
    ("bugfix", &[" bug", " corrij", " corrig", " fix", " quebrad", " falhando", " crash", " exception", " nao funciona", " broken", " regress"]),
    ("refactor", &[" refator", " refactor", " limpe ", " cleanup", " simplifi", " renome", " rename", " reorganiz", " extrair", " extract"]),
    ("design", &[" ux ", " wireframe", " mockup", " prototipo", " design visual", " paleta"]),
    ("feature", &[" implement", " crie ", " criar ", " cria ", " adicion", " add ", " build ", " create ", " nova funcionalidade", " feature", " desenvolv", " integre", " integrar", " suporte a"]),
    ("question", &[" como ", " por que", " porque ", " o que ", " explique", " explica", " explain", " what ", " why ", " how ", " onde ", " where ", " qual ", " quais ", " entenda", " resuma", " summarize"]),
];

const DOMAINS: &[(&str, &[&str])] = &[
    ("security", &[" seguranca", " vulnerab", " xss", " csrf", " sql injection", " owasp", " security", " pentest", " vazamento", " segredo", " secrets"]),
    ("database", &[" banco de dados", " sql", " migration", " migraca", " schema", " query", " queries", " postgres", " mysql", " sqlite", " supabase", " indice", " index", " prisma", " drizzle", " tabela"]),
    ("devops", &[" deploy", " docker", " ci ", " pipeline", " kubernetes", " github actions", " vercel", " nginx", " terraform", " infra", " cloudflare"]),
    ("frontend", &[" frontend", " front end", " ui ", " interface", " css", " tailwind", " componente", " component", " react", " tela", " layout", " responsiv", " botao", " button", " pagina", " page ", " modal", " formulario"]),
    ("backend", &[" api", " endpoint", " backend", " servidor", " server", " rota", " route", " webhook", " fastapi", " express", " servico", " service"]),
];

const LARGE: &[&str] = &[
    " arquitetura", " sistema inteiro", " todo o projeto", " migrar", " migrate", " reescrev", " rewrite", " end to end", " completo", " completa",
    " varios modulos", " multiple", " pagamento", " payments", " autenticacao", " authentication", " multi tenant", " do zero", " from scratch",
];
const SMALL: &[&str] = &[" typo", " simples", " pequen", " rapid", " quick", " trivial", " uma linha", " one line", " um arquivo", " single file", " so o ", " apenas"];

fn hits<'a>(text: &str, words: &'a [&'a str]) -> Vec<&'a str> {
    words.iter().copied().filter(|w| text.contains(w)).collect()
}

pub fn classify(raw: &str) -> Classification {
    let text = fold(raw);
    let mut signals: Vec<String> = Vec::new();
    // Activity: first non-question activity with hits wins (list order = priority);
    // a pure question stays read-only.
    let mut activity = None;
    for (name, words) in ACTIVITIES {
        let h = hits(&text, words);
        if !h.is_empty() {
            signals.extend(h.iter().map(|w| w.trim().to_string()));
            activity = Some(*name);
            break;
        }
    }
    let activity = activity.unwrap_or(if raw.trim_end().ends_with('?') { "question" } else { "feature" });
    let mut domain = None;
    let mut best = 0;
    for (name, words) in DOMAINS {
        let h = hits(&text, words);
        if h.len() > best {
            best = h.len();
            domain = Some(*name);
            signals.retain(|s| !h.iter().any(|w| w.trim() == s));
            signals.extend(h.iter().map(|w| w.trim().to_string()));
        }
    }
    let words = text.split_whitespace().count();
    let mut level: i32 = if words < 25 { 0 } else if words < 120 { 1 } else { 2 };
    let large = hits(&text, LARGE);
    let small = hits(&text, SMALL);
    if !large.is_empty() {
        level += 1;
        signals.extend(large.iter().map(|w| w.trim().to_string()));
    }
    if !small.is_empty() {
        level -= 1;
        signals.extend(small.iter().map(|w| w.trim().to_string()));
    }
    let complexity = ["small", "medium", "large"][level.clamp(0, 2) as usize].to_string();
    let writes = matches!(activity, "feature" | "bugfix" | "refactor" | "tests" | "docs");
    signals.dedup();
    Classification { activity: activity.into(), domain: domain.map(String::from), complexity, writes, signals }
}

// ── Inputs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub logged_out: bool,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub permission: String,
    pub max_turns: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Named {
    pub id: String,
    pub name: String,
    /// Text searched for relevance (name, description, template…).
    pub haystack: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageStat {
    pub provider_id: String,
    pub model: Option<String>,
    pub runs: i64,
    pub completed: i64,
    pub avg_duration_ms: Option<i64>,
    /// Only runs where the CLI reported a cost.
    pub cost_runs: i64,
    pub avg_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Preference {
    Economy,
    #[default]
    Balanced,
    Quality,
}

pub struct HarnessInput<'a> {
    pub text: &'a str,
    pub preference: Preference,
    pub providers: &'a [ProviderInfo],
    pub profiles: &'a [ProfileInfo],
    pub skills: &'a [Named],
    pub mcp: &'a [Named],
    pub usage: &'a [UsageStat],
}

// ── Recommendation ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub classification: Classification,
    pub profile: Option<Pick>,
    pub provider_id: Option<String>,
    /// None = the CLI's default model.
    pub model: Option<String>,
    pub permission: String,
    pub max_turns: i64,
    pub use_worktree: bool,
    pub skills: Vec<Pick>,
    pub mcp: Vec<Pick>,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

/// Agent role for an activity/domain.
pub fn role_for(c: &Classification) -> &'static str {
    let d = c.domain.as_deref();
    match c.activity.as_str() {
        "review" => {
            if d == Some("security") {
                "security"
            } else {
                "reviewer"
            }
        }
        "plan" => "architect",
        "docs" => "docs",
        "tests" => "qa",
        "design" => "designer",
        "question" => match d {
            Some("security") => "security",
            Some("frontend") | Some("backend") | Some("database") | Some("devops") => "architect",
            _ => "researcher",
        },
        _ => match d {
            Some("frontend") => "frontend",
            Some("backend") => "backend",
            Some("database") => "database",
            Some("devops") => "devops",
            // Security *changes* are implementation work; the read-only
            // Security Auditor reviews them.
            Some("security") => "backend",
            _ => "executor",
        },
    }
}

/// Claude model alias for the task (real CLI aliases only).
pub fn claude_tier(c: &Classification, pref: Preference) -> &'static str {
    const TIERS: [&str; 3] = ["haiku", "sonnet", "opus"];
    let mut i: i32 = match c.complexity.as_str() {
        "small" => 0,
        "medium" => 1,
        _ => 2,
    };
    i += match pref {
        Preference::Economy => -1,
        Preference::Balanced => 0,
        Preference::Quality => 1,
    };
    // Judgement-heavy work never goes below sonnet.
    let floor = if matches!(c.activity.as_str(), "review" | "plan") || c.domain.as_deref() == Some("security") { 1 } else { 0 };
    TIERS[i.clamp(floor, 2) as usize]
}

pub fn turns_for(c: &Classification, pref: Preference) -> i64 {
    let base = match c.complexity.as_str() {
        "small" => 12,
        "medium" => 30,
        _ => 60,
    };
    match pref {
        Preference::Economy => base * 2 / 3,
        Preference::Balanced => base,
        Preference::Quality => base * 3 / 2,
    }
}

fn relevant(text: &str, items: &[Named], max: usize) -> Vec<Pick> {
    let t = fold(text);
    let mut scored: Vec<(usize, &Named)> = items
        .iter()
        .map(|n| {
            let hay = fold(&n.haystack);
            let name_hit = fold(&n.name).split_whitespace().filter(|w| w.len() >= 3).any(|w| t.contains(&format!(" {w} ")));
            let words = t.split_whitespace().filter(|w| w.len() >= 5).filter(|w| hay.contains(&format!(" {w}"))).count();
            (words + if name_hit { 2 } else { 0 }, n)
        })
        .filter(|(s, _)| *s >= 2)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    scored.into_iter().take(max).map(|(_, n)| Pick { id: n.id.clone(), name: n.name.clone() }).collect()
}

fn stat_line(s: &UsageStat) -> String {
    let rate = if s.runs > 0 { s.completed * 100 / s.runs } else { 0 };
    let mut line = format!(
        "{}{}: {} execução(ões) nesta máquina, {}% concluídas",
        s.provider_id,
        s.model.as_deref().map(|m| format!(" ({m})")).unwrap_or_default(),
        s.runs,
        rate
    );
    if let Some(d) = s.avg_duration_ms {
        line.push_str(&format!(", {:.0}s em média", d as f64 / 1000.0));
    }
    if let Some(c) = s.avg_cost_usd {
        line.push_str(&format!(", custo informado médio US$ {c:.4} ({} exec.)", s.cost_runs));
    }
    line
}

pub fn recommend(input: &HarnessInput) -> Recommendation {
    let c = classify(input.text);
    let mut reasons = Vec::new();
    let mut warnings = Vec::new();
    let kind_pt = match c.activity.as_str() {
        "question" => "pergunta/explicação",
        "review" => "revisão",
        "plan" => "planejamento/arquitetura",
        "docs" => "documentação",
        "tests" => "testes",
        "bugfix" => "correção de bug",
        "refactor" => "refatoração",
        "design" => "design",
        _ => "implementação",
    };
    let size_pt = match c.complexity.as_str() {
        "small" => "pequena",
        "medium" => "média",
        _ => "grande",
    };
    reasons.push(format!(
        "Tarefa de {kind_pt}{}, porte {size_pt}{}.",
        c.domain.as_deref().map(|d| format!(" ({d})")).unwrap_or_default(),
        if c.signals.is_empty() { String::new() } else { format!(" — sinais: {}", c.signals.iter().take(6).cloned().collect::<Vec<_>>().join(", ")) }
    ));

    // Providers usable now.
    let usable: Vec<&ProviderInfo> = input.providers.iter().filter(|p| p.installed && !p.logged_out).collect();
    let unreliable = |pid: &str| input.usage.iter().filter(|u| u.provider_id == pid).fold((0, 0), |(r, ok), u| (r + u.runs, ok + u.completed));
    let healthy = |pid: &str| {
        let (r, ok) = unreliable(pid);
        r < 4 || ok * 2 >= r
    };

    // Profile by role.
    let role = role_for(&c);
    let profile = input.profiles.iter().find(|p| p.role == role && usable.iter().any(|u| u.id == p.provider_id)).or_else(|| input.profiles.iter().find(|p| p.role == role));
    if let Some(p) = profile {
        reasons.push(format!("Perfil “{}” (papel {role}) é o especialista para esse tipo de tarefa.", p.name));
    } else {
        warnings.push(format!("Nenhum perfil com o papel “{role}” — crie um em Agentes."));
    }

    // Provider: the profile's, if usable and healthy; else the best usable one.
    let mut provider_id = profile.map(|p| p.provider_id.clone()).filter(|pid| usable.iter().any(|u| &u.id == pid));
    if let Some(pid) = &provider_id {
        if !healthy(pid) {
            let alt = usable.iter().filter(|u| &u.id != pid && healthy(&u.id)).max_by_key(|u| (u.verified, u.id == "claude"));
            if let Some(a) = alt {
                warnings.push(format!("{pid} teve mais falhas que sucessos nas últimas execuções; sugerindo {} no lugar.", a.name));
                provider_id = Some(a.id.clone());
            }
        }
    } else {
        provider_id = usable.iter().filter(|u| healthy(&u.id)).max_by_key(|u| (u.verified, u.id == "claude")).map(|u| u.id.clone());
        if let (Some(p), Some(pid)) = (profile, &provider_id) {
            warnings.push(format!("O provider do perfil ({}) não está disponível; usando {pid}.", p.provider_id));
        }
    }
    match provider_id.as_deref().and_then(|pid| input.providers.iter().find(|p| p.id == pid)) {
        Some(p) if p.verified => reasons.push(format!("{} está instalado e verificado nesta máquina.", p.name)),
        Some(p) => warnings.push(format!("{} está instalado, mas ainda não foi verificado (Providers → Verificar).", p.name)),
        None => warnings.push("Nenhum provider instalado e autenticado — veja a tela Providers.".into()),
    }

    // Model + effort.
    let profile_model = profile.and_then(|p| p.model.clone()).filter(|_| profile.map(|p| Some(&p.provider_id)) == Some(provider_id.as_ref()));
    let model = match provider_id.as_deref() {
        Some("claude") => {
            let tier = claude_tier(&c, input.preference);
            reasons.push(format!(
                "Modelo {tier}: {}.",
                match tier {
                    "haiku" => "rápido e econômico para tarefas pequenas",
                    "sonnet" => "equilíbrio entre qualidade e consumo",
                    _ => "máxima capacidade para trabalho grande ou crítico",
                }
            ));
            if let Some(m) = &profile_model {
                if m != tier {
                    reasons.push(format!("(O perfil fixa “{m}”; aplicar a recomendação usa {tier} só nesta execução.)"));
                }
            }
            Some(tier.to_string())
        }
        Some(_) => {
            reasons.push("Modelo: o padrão configurado no CLI (o FORGE não escolhe modelos que não pode validar neste provider).".into());
            profile_model.clone()
        }
        None => None,
    };
    let max_turns = turns_for(&c, input.preference);
    reasons.push(format!("Esforço: até {max_turns} turnos (limite real passado ao agente)."));

    // Permission: least privilege, never full.
    let permission = if c.writes { "edit" } else { "read" }.to_string();
    reasons.push(if c.writes {
        "Permissão “pode editar”: a tarefa altera arquivos (nunca “acesso total” automaticamente).".into()
    } else {
        "Permissão “somente leitura”: a tarefa não precisa alterar arquivos.".into()
    });
    let use_worktree = c.writes && c.complexity == "large";
    if use_worktree {
        reasons.push("Worktree isolada: mudança grande, fica numa branch própria até você revisar.".into());
    }

    let skills = relevant(input.text, input.skills, 3);
    if !skills.is_empty() {
        reasons.push(format!("Skills relevantes: {}.", skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let mut mcp = relevant(input.text, input.mcp, 3);
    if !mcp.is_empty() && provider_id.as_deref() != Some("claude") {
        warnings.push(format!("MCP por execução só funciona no Claude Code; {} ficaria(m) de fora.", mcp.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")));
        mcp.clear();
    } else if !mcp.is_empty() {
        reasons.push(format!("Ferramentas MCP relevantes: {}.", mcp.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")));
    }

    if let Some(pid) = &provider_id {
        let mut stats: Vec<&UsageStat> = input.usage.iter().filter(|u| &u.provider_id == pid).collect();
        stats.sort_by_key(|u| -u.runs);
        if let Some(s) = stats.iter().find(|s| s.model == model).or(stats.first()) {
            reasons.push(format!("Histórico: {}.", stat_line(s)));
        } else {
            reasons.push("Sem histórico de execuções para esse provider ainda.".into());
        }
    }

    Recommendation {
        classification: c,
        profile: profile.map(|p| Pick { id: p.id.clone(), name: p.name.clone() }),
        provider_id,
        model,
        permission,
        max_turns,
        use_worktree,
        skills,
        mcp,
        reasons,
        warnings,
    }
}

/// Usage per provider/model from this machine's runs (last 30 days).
pub fn usage(conn: &Connection, since_ms: i64) -> AppResult<Vec<UsageStat>> {
    let mut st = conn.prepare(
        "SELECT provider_id, model, COUNT(*),
                SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END),
                CAST(AVG(duration_ms) AS INTEGER),
                COUNT(cost_usd), AVG(cost_usd)
         FROM agent_runs WHERE started_at >= ?1 AND status != 'running'
         GROUP BY provider_id, model ORDER BY COUNT(*) DESC",
    )?;
    let rows = st
        .query_map([since_ms], |r| {
            Ok(UsageStat {
                provider_id: r.get(0)?,
                model: r.get(1)?,
                runs: r.get(2)?,
                completed: r.get(3)?,
                avg_duration_ms: r.get(4)?,
                cost_runs: r.get(5)?,
                avg_cost_usd: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(s: &str) -> (String, Option<String>, String, bool) {
        let c = classify(s);
        (c.activity, c.domain, c.complexity, c.writes)
    }

    #[test]
    fn classifies_common_tasks() {
        assert_eq!(cls("Corrija o bug do botão de login que não funciona no Safari"), ("bugfix".into(), Some("frontend".into()), "small".into(), true));
        assert_eq!(cls("Revise o código do endpoint /api/orders"), ("review".into(), Some("backend".into()), "small".into(), false));
        assert_eq!(cls("Explique como funciona a autenticação?").0, "question");
        assert_eq!(cls("Escreva testes para o serviço de pagamentos").0, "tests");
        assert_eq!(cls("Crie a migration da tabela de pedidos no Postgres"), ("feature".into(), Some("database".into()), "small".into(), true));
        assert_eq!(cls("Atualize o README com instruções de deploy").0, "docs");
        assert_eq!(cls("Planeje a arquitetura do módulo de notificações").0, "plan");
        let big = cls("Implementar o sistema de pagamentos completo com Stripe, webhooks, reembolsos e painel administrativo");
        assert_eq!((big.0.as_str(), big.2.as_str()), ("feature", "medium"));
        assert_eq!(cls("Corrija um typo simples no rodapé").2, "small");
        assert_eq!(cls("Audite a segurança do fluxo de upload contra XSS").1.as_deref(), Some("security"));
    }

    #[test]
    fn complexity_grows_with_length_and_markers() {
        let long = "Implemente ".to_string() + &"a funcionalidade com vários detalhes e validações ".repeat(20);
        assert_eq!(classify(&long).complexity, "large");
        assert_eq!(classify("Migrar todo o projeto para o novo framework").complexity, "medium");
    }

    #[test]
    fn roles_and_tiers() {
        let c = classify("Revise a segurança do login");
        assert_eq!(role_for(&c), "security");
        assert_eq!(claude_tier(&c, Preference::Economy), "sonnet", "security never below sonnet");
        let small = classify("Corrija um typo no botão");
        assert_eq!(role_for(&small), "frontend");
        assert_eq!(claude_tier(&small, Preference::Balanced), "haiku");
        assert_eq!(claude_tier(&small, Preference::Quality), "sonnet");
        let long = "Implemente ".to_string() + &"a funcionalidade com vários detalhes e validações ".repeat(20);
        assert_eq!(claude_tier(&classify(&long), Preference::Balanced), "opus");
        assert_eq!(claude_tier(&classify(&long), Preference::Economy), "sonnet");
        assert!(turns_for(&small, Preference::Economy) < turns_for(&small, Preference::Quality));
    }

    fn providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo { id: "claude".into(), name: "Claude Code".into(), installed: true, logged_out: false, verified: true },
            ProviderInfo { id: "codex".into(), name: "Codex".into(), installed: true, logged_out: true, verified: false },
            ProviderInfo { id: "gemini".into(), name: "Gemini".into(), installed: false, logged_out: true, verified: false },
        ]
    }

    fn profiles() -> Vec<ProfileInfo> {
        let p = |id: &str, role: &str, provider: &str, perm: &str| ProfileInfo {
            id: id.into(),
            name: id.into(),
            role: role.into(),
            provider_id: provider.into(),
            model: None,
            permission: perm.into(),
            max_turns: None,
        };
        vec![p("Frontend", "frontend", "claude", "edit"), p("Reviewer", "reviewer", "claude", "read"), p("Backend", "backend", "codex", "edit"), p("Executor", "executor", "claude", "edit")]
    }

    #[test]
    fn recommends_least_privilege_real_models_and_usable_providers() {
        let skills = vec![Named { id: "s1".into(), name: "Tailwind UI".into(), haystack: "Tailwind UI conventions for components and responsive layout".into() }];
        let mcp = vec![
            Named { id: "m1".into(), name: "github".into(), haystack: "github GitHub template github".into() },
            Named { id: "m2".into(), name: "supabase".into(), haystack: "supabase Supabase".into() },
        ];
        let usage = vec![UsageStat { provider_id: "claude".into(), model: Some("haiku".into()), runs: 5, completed: 5, avg_duration_ms: Some(4000), cost_runs: 5, avg_cost_usd: Some(0.01) }];
        let pr = providers();
        let pf = profiles();
        let r = recommend(&HarnessInput {
            text: "Corrija o layout responsivo do componente de tabela com Tailwind",
            preference: Preference::Balanced,
            providers: &pr,
            profiles: &pf,
            skills: &skills,
            mcp: &mcp,
            usage: &usage,
        });
        assert_eq!(r.profile.as_ref().unwrap().id, "Frontend");
        assert_eq!(r.provider_id.as_deref(), Some("claude"));
        assert_eq!(r.model.as_deref(), Some("haiku"));
        assert_eq!(r.permission, "edit");
        assert!(!r.use_worktree);
        assert_eq!(r.skills, vec![Pick { id: "s1".into(), name: "Tailwind UI".into() }]);
        assert!(r.mcp.is_empty(), "no MCP keyword in the task");
        assert!(r.reasons.iter().any(|x| x.contains("custo informado")), "{:?}", r.reasons);

        // Backend profile is on Codex (logged out) → falls back to an usable provider and says so.
        let b = recommend(&HarnessInput { text: "Crie um endpoint de API para pedidos e publique no github", preference: Preference::Balanced, providers: &pr, profiles: &pf, skills: &[], mcp: &mcp, usage: &[] });
        assert_eq!(b.profile.as_ref().unwrap().id, "Backend");
        assert_eq!(b.provider_id.as_deref(), Some("claude"));
        assert!(b.warnings.iter().any(|w| w.contains("codex")), "{:?}", b.warnings);
        assert_eq!(b.mcp, vec![Pick { id: "m1".into(), name: "github".into() }]);
        assert!(b.reasons.iter().any(|x| x.contains("Sem histórico")));

        // Reviews are read-only and never below sonnet.
        let rv = recommend(&HarnessInput { text: "Revise o PR de autenticação", preference: Preference::Economy, providers: &pr, profiles: &pf, skills: &[], mcp: &[], usage: &[] });
        assert_eq!((rv.permission.as_str(), rv.model.as_deref()), ("read", Some("sonnet")));
    }

    #[test]
    fn unhealthy_history_and_no_providers_are_reported() {
        let pr = vec![
            ProviderInfo { id: "claude".into(), name: "Claude Code".into(), installed: true, logged_out: false, verified: true },
            ProviderInfo { id: "gemini".into(), name: "Gemini".into(), installed: true, logged_out: false, verified: true },
        ];
        let pf = profiles();
        let usage = vec![UsageStat { provider_id: "claude".into(), model: None, runs: 6, completed: 1, avg_duration_ms: None, cost_runs: 0, avg_cost_usd: None }];
        let r = recommend(&HarnessInput { text: "Corrija o botão", preference: Preference::Balanced, providers: &pr, profiles: &pf, skills: &[], mcp: &[], usage: &usage });
        assert_eq!(r.provider_id.as_deref(), Some("gemini"));
        assert_eq!(r.model, None, "no invented model names for gemini");
        assert!(r.warnings.iter().any(|w| w.contains("falhas")));
        let none = recommend(&HarnessInput { text: "Corrija o botão", preference: Preference::Balanced, providers: &[], profiles: &pf, skills: &[], mcp: &[], usage: &[] });
        assert_eq!(none.provider_id, None);
        assert!(none.warnings.iter().any(|w| w.contains("Nenhum provider")));
    }

    #[test]
    fn usage_is_aggregated_from_real_runs() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let stats = db
            .with(|c| {
                c.execute_batch(
                    "INSERT INTO workspaces (id, name, path, created_at) VALUES ('w','w','/tmp/w',0);
                     INSERT INTO agent_runs (id, workspace_id, provider_id, model, permission, prompt, status, cwd, started_at, duration_ms, cost_usd) VALUES
                       ('a','w','claude','haiku','read','p','completed','/tmp',100,2000,0.01),
                       ('b','w','claude','haiku','read','p','failed','/tmp',100,4000,NULL),
                       ('c','w','claude','haiku','read','p','running','/tmp',100,NULL,NULL),
                       ('d','w','codex',NULL,'read','p','completed','/tmp',1,1000,NULL);",
                )?;
                usage(c, 50)
            })
            .unwrap();
        assert_eq!(stats.len(), 1);
        let s = &stats[0];
        assert_eq!((s.runs, s.completed, s.avg_duration_ms, s.cost_runs), (2, 1, Some(3000), 1));
        assert!((s.avg_cost_usd.unwrap() - 0.01).abs() < 1e-9);
    }

    /// Real provider detection on this machine (no prompts sent).
    #[test]
    #[ignore = "inspects the installed CLIs"]
    fn live_recommendation_uses_only_real_providers() {
        let providers: Vec<ProviderInfo> = crate::providers::PROVIDERS
            .iter()
            .map(|p| crate::providers::status(p, None))
            .map(|st| ProviderInfo { id: st.id, name: st.name, installed: st.installed, logged_out: st.auth.state == "logged_out", verified: st.verified })
            .collect();
        eprintln!("{:?}", providers.iter().map(|p| (&p.id, p.installed, p.logged_out)).collect::<Vec<_>>());
        let db = crate::db::Db::open_in_memory().unwrap();
        let profiles: Vec<ProfileInfo> = db
            .with(|c| {
                crate::agents::profiles::ensure_builtins(c, "claude")?;
                crate::agents::profiles::list(c)
            })
            .unwrap()
            .into_iter()
            .map(|p| ProfileInfo { id: p.id, name: p.name, role: p.role, provider_id: p.provider_id, model: p.model, permission: p.permission, max_turns: p.max_turns })
            .collect();
        let r = recommend(&HarnessInput { text: "Corrija um typo no botão de login", preference: Preference::Balanced, providers: &providers, profiles: &profiles, skills: &[], mcp: &[], usage: &[] });
        eprintln!("{:#?}", r);
        let chosen = r.provider_id.clone().expect("a provider is usable here");
        let p = providers.iter().find(|p| p.id == chosen).unwrap();
        assert!(p.installed && !p.logged_out);
        assert_eq!(r.profile.unwrap().id, "builtin:frontend");
        if chosen == "claude" {
            assert_eq!(r.model.as_deref(), Some("haiku"));
        }
    }
}
