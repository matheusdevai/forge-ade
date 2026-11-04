//! Maestro: review verdicts, failure policy and the final mission report.
//! Pure functions — the orchestration lives in `engine.rs`.

use serde::Serialize;
use serde_json::Value;

use super::tasks::Task;
use crate::db::missions::Mission;
use crate::git::Commit;

pub const REVIEW_OPEN: &str = "<forge-review>";
pub const REVIEW_CLOSE: &str = "</forge-review>";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Review {
    pub approve: bool,
    pub summary: String,
    pub issues: Vec<String>,
}

impl Review {
    /// Text stored on the task and sent back to the agent on a retry.
    pub fn as_feedback(&self) -> String {
        let mut s = self.summary.trim().to_string();
        for i in &self.issues {
            s.push_str(&format!("\n- {i}"));
        }
        s
    }
}

pub fn review_prompt(mission_title: &str, task: &Task, base_commit: Option<&str>) -> String {
    let range = base_commit.map(|b| format!("`git diff {b}..HEAD` and `git log {b}..HEAD`")).unwrap_or_else(|| "`git log` and `git show`".into());
    format!(
        "You are the reviewer for the mission \"{mission_title}\". Another agent just completed the task below in this git worktree.\n\n\
TASK: {title}\n\n{desc}\n\nACCEPTANCE CRITERIA:\n{acc}\n\n\
Inspect the changes with {range}, read the relevant files and, if cheap and read-only, run existing checks. \
Approve only if the acceptance criteria are met and there are no blocking bugs or security problems. \
Be specific in issues (file, problem, fix). Do not modify files.\n\n\
End your reply with the verdict exactly in this format:\n\
{REVIEW_OPEN}{{\"verdict\": \"approve\" | \"reject\", \"summary\": \"one short paragraph\", \"issues\": [\"...\"]}}{REVIEW_CLOSE}",
        title = task.title,
        desc = task.description,
        acc = if task.acceptance.trim().is_empty() { "(none specified — judge correctness and completeness)" } else { task.acceptance.trim() },
    )
}

pub fn parse_review(text: &str) -> Option<Review> {
    let body = match (text.rfind(REVIEW_OPEN), text.rfind(REVIEW_CLOSE)) {
        (Some(a), Some(b)) if b > a => &text[a + REVIEW_OPEN.len()..b],
        _ => text,
    };
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    let v: Value = serde_json::from_str(&body[start..=end]).ok()?;
    let verdict = v.get("verdict").and_then(Value::as_str)?.trim().to_lowercase();
    let approve = match verdict.as_str() {
        "approve" | "approved" | "accept" | "pass" => true,
        "reject" | "rejected" | "changes_requested" | "fail" => false,
        _ => return None,
    };
    Some(Review {
        approve,
        summary: v.get("summary").and_then(Value::as_str).unwrap_or_default().trim().chars().take(2000).collect(),
        issues: v
            .get("issues")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(|s| s.chars().take(500).collect()).take(20).collect())
            .unwrap_or_default(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureAction {
    Retry,
    Reassign(String),
    Escalate,
}

/// Agentic policy: retry until `max_attempts`; then reassign once to another
/// available provider; then ask the human.
pub fn decide_on_failure(attempts: i64, max_attempts: i64, already_reassigned: bool, candidate: Option<String>) -> FailureAction {
    if attempts < max_attempts {
        FailureAction::Retry
    } else if !already_reassigned {
        candidate.map(FailureAction::Reassign).unwrap_or(FailureAction::Escalate)
    } else {
        FailureAction::Escalate
    }
}

pub struct ReportTask<'a> {
    pub task: &'a Task,
    pub profile: String,
    pub provider: String,
    pub commits: Vec<Commit>,
}

const STATUS_PT: &[(&str, &str)] = &[
    ("approved", "aprovada"),
    ("skipped", "pulada"),
    ("failed", "falhou"),
    ("rejected", "rejeitada"),
    ("cancelled", "cancelada"),
    ("review", "em revisão"),
    ("running", "em execução"),
    ("pending", "pendente"),
];

fn pt(status: &str) -> &str {
    STATUS_PT.iter().find(|(k, _)| *k == status).map(|(_, v)| *v).unwrap_or(status)
}

/// Deterministic markdown report built only from recorded facts (no LLM).
pub fn build_report(m: &Mission, tasks: &[ReportTask], runs: i64, input_tokens: i64, output_tokens: i64, cost: Option<f64>, cost_runs: i64) -> String {
    let mut s = format!("# Relatório da missão — {}\n\n", m.title);
    s.push_str(&format!(
        "- **Modo:** {}\n- **Base:** {} @ {}\n- **Tarefas:** {} ({} aprovadas, {} puladas)\n- **Execuções de agente:** {runs}\n- **Tokens:** {input_tokens} entrada → {output_tokens} saída\n",
        m.mode,
        m.base_branch.as_deref().unwrap_or("HEAD"),
        m.base_commit.as_deref().map(|c| &c[..7.min(c.len())]).unwrap_or("?"),
        tasks.len(),
        tasks.iter().filter(|t| t.task.status == "approved").count(),
        tasks.iter().filter(|t| t.task.status == "skipped").count(),
    ));
    match cost {
        Some(c) => s.push_str(&format!("- **Custo informado pelos CLIs:** US$ {c:.4} ({cost_runs} de {runs} execuções informaram; valores não estimados)\n")),
        None => s.push_str("- **Custo:** nenhum CLI informou custo\n"),
    }
    s.push_str("\n## Briefing\n\n");
    s.push_str(m.briefing.trim());
    s.push_str("\n\n## Tarefas\n");
    for rt in tasks {
        let t = rt.task;
        s.push_str(&format!("\n### {} — {} ({})\n\n", t.key, t.title, pt(&t.status)));
        s.push_str(&format!("- Agente: {} via {}", rt.profile, rt.provider));
        if t.provider_override.is_some() {
            s.push_str(" (reatribuída pelo Maestro)");
        }
        s.push_str(&format!("\n- Tentativas: {}\n", t.attempts));
        if let Some(b) = &t.worktree_branch {
            s.push_str(&format!("- Branch: `{b}`{}\n", if t.integrated_at.is_some() { " (integrada)" } else { "" }));
        }
        if !rt.commits.is_empty() {
            s.push_str("- Commits:\n");
            for c in rt.commits.iter().take(20) {
                s.push_str(&format!("  - `{}` {}\n", c.short, c.subject));
            }
        }
        if let Some(sum) = t.review_summary.as_deref().filter(|x| !x.trim().is_empty()) {
            let verdict = match t.review_verdict.as_deref() {
                Some("approve") => "aprovou",
                Some("reject") => "rejeitou",
                _ => "comentou",
            };
            s.push_str(&format!("- Revisão do Maestro ({verdict}): {}\n", sum.replace('\n', "\n  ")));
        }
    }
    let open: Vec<&ReportTask> = tasks.iter().filter(|t| !matches!(t.task.status.as_str(), "approved")).collect();
    let flagged: Vec<&ReportTask> = tasks
        .iter()
        .filter(|t| t.task.status == "approved" && t.task.review_verdict.as_deref() == Some("reject"))
        .collect();
    s.push_str("\n## Pendências e riscos\n\n");
    if open.is_empty() && flagged.is_empty() {
        s.push_str("Nenhuma pendência registrada. Revise o diff antes de integrar e rode a suíte de testes do projeto.\n");
    } else {
        for t in open {
            s.push_str(&format!("- {} — {}: {}\n", t.task.key, t.task.title, pt(&t.task.status)));
        }
        for t in flagged {
            s.push_str(&format!("- {} — aprovada por você apesar da rejeição do Maestro\n", t.task.key));
        }
    }
    s.push_str(&format!("\n## Integração\n\n{}\n", if m.integrated_at.is_some() { "Branches integradas na base." } else { "Ainda não integrada: use “Integrar” na tela da missão." }));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(status: &str) -> Task {
        Task {
            id: "t".into(),
            mission_id: "m".into(),
            position: 0,
            key: "api".into(),
            title: "Build API".into(),
            description: "d".into(),
            acceptance: "tests pass".into(),
            profile_id: None,
            depends_on: vec![],
            status: status.into(),
            run_id: None,
            worktree_path: None,
            worktree_branch: Some("forge/m/api-1".into()),
            attempts: 2,
            feedback: None,
            integrated_at: None,
            created_at: 0,
            updated_at: 0,
            base_commit: Some("abcdef1234".into()),
            review_run_id: None,
            review_verdict: Some("approve".into()),
            review_summary: Some("Looks good".into()),
            provider_override: None,
        }
    }

    #[test]
    fn parses_review_verdicts() {
        let r = parse_review("All fine.\n<forge-review>{\"verdict\":\"approve\",\"summary\":\"ok\",\"issues\":[]}</forge-review>").unwrap();
        assert!(r.approve);
        let r = parse_review("<forge-review>{\"verdict\": \"REJECT\", \"summary\": \"missing tests\", \"issues\": [\"no test for --name\", \"typo\"]}</forge-review>").unwrap();
        assert!(!r.approve);
        assert_eq!(r.as_feedback(), "missing tests\n- no test for --name\n- typo");
        assert!(parse_review("no verdict").is_none());
        assert!(parse_review("<forge-review>{\"verdict\":\"maybe\"}</forge-review>").is_none());
        // The last block wins (the prompt itself may echo the format).
        let echoed = "format: <forge-review>{\"verdict\": \"approve\" | \"reject\"}</forge-review> ... final: <forge-review>{\"verdict\":\"reject\",\"summary\":\"x\"}</forge-review>";
        assert!(!parse_review(echoed).unwrap().approve);
    }

    #[test]
    fn failure_policy() {
        assert_eq!(decide_on_failure(1, 3, false, Some("codex".into())), FailureAction::Retry);
        assert_eq!(decide_on_failure(3, 3, false, Some("codex".into())), FailureAction::Reassign("codex".into()));
        assert_eq!(decide_on_failure(3, 3, false, None), FailureAction::Escalate);
        assert_eq!(decide_on_failure(4, 3, true, Some("gemini".into())), FailureAction::Escalate);
    }

    #[test]
    fn report_contains_facts_only() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let m = db
            .with(|c| crate::db::missions::create(c, &crate::db::missions::NewMission { workspace_id: None, title: "App".into(), briefing: "Build it".into(), mode: "agentic".into() }))
            .unwrap();
        let ok = task("approved");
        let mut failed = task("failed");
        failed.key = "ui".into();
        let commits = vec![Commit { sha: "a".repeat(40), short: "aaaaaaa".into(), author: "x".into(), email: "x".into(), time: 0, refs: vec![], subject: "feat: api".into() }];
        let r = build_report(
            &m,
            &[
                ReportTask { task: &ok, profile: "Backend Engineer".into(), provider: "Claude Code".into(), commits },
                ReportTask { task: &failed, profile: "Frontend Engineer".into(), provider: "Claude Code".into(), commits: vec![] },
            ],
            5,
            100,
            50,
            None,
            0,
        );
        assert!(r.contains("# Relatório da missão — App"));
        assert!(r.contains("`aaaaaaa` feat: api"));
        assert!(r.contains("Revisão do Maestro (aprovou): Looks good"));
        assert!(r.contains("ui — Build API: falhou"));
        assert!(r.contains("nenhum CLI informou custo"));
    }
}
