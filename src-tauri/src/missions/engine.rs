//! Mission orchestration.
//!
//! Lifecycle: draft → (plan) → planned → (start) → running ⇄ paused → completed → (integrate)
//! Tasks:     pending → running → review → approved | rejected
//!                              ↘ failed / cancelled          (retry / skip)
//!
//! Isolation: every task runs in its own git worktree whose branch starts from
//! the mission base commit plus the branches of its dependencies, so tasks
//! running in parallel never edit the same checkout. Results are committed
//! on the task branch; nothing touches the user's branch until `integrate`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
#[cfg(test)]
use serde_json::Value;

use super::maestro::{self, FailureAction};
use super::tasks::{self, Task};
use crate::agents::profiles;
use crate::agents::runner::{RunManager, RunRequest};
use crate::agents::runs::{self, Run};
use crate::db::{missions, workspaces};
use crate::error::{AppError, AppResult};
use crate::{git, providers};

pub type Notifier = Arc<dyn Fn(&str) + Send + Sync>;
/// Finds another installed + logged-in provider (excluding the given one) for reassignment.
pub type Probe = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn default_probe() -> Probe {
    Arc::new(|current: &str| {
        providers::PROVIDERS.iter().filter(|p| p.id != current).find_map(|p| {
            let st = providers::status(p, None);
            (st.installed && st.auth.state == "logged_in").then(|| p.id.to_string())
        })
    })
}

#[derive(Clone)]
pub struct MissionEngine {
    inner: Arc<Inner>,
}

struct Inner {
    db_path: PathBuf,
    worktrees: PathBuf,
    runs: RunManager,
    notify: Notifier,
    lock: Mutex<()>,
    /// Test hook: replaces the provider binary.
    program: Option<PathBuf>,
    probe: Probe,
    secrets: crate::mcp::secrets::Secrets,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub runs: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: Option<f64>,
    pub cost_reported_runs: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionDetail {
    pub mission: missions::Mission,
    pub tasks: Vec<Task>,
    pub events: Vec<tasks::MissionEvent>,
    pub runs: Vec<Run>,
    pub totals: Totals,
}

fn open(path: &Path) -> AppResult<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    Ok(c)
}

fn slug(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    out.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(32).collect()
}

impl MissionEngine {
    pub fn new(db_path: PathBuf, worktrees: PathBuf, runs: RunManager, notify: Notifier) -> Self {
        Self::with_program(db_path, worktrees, runs, notify, None)
    }

    pub fn with_program(db_path: PathBuf, worktrees: PathBuf, runs: RunManager, notify: Notifier, program: Option<PathBuf>) -> Self {
        Self::with_options(db_path, worktrees, runs, notify, program, None)
    }

    pub fn with_options(db_path: PathBuf, worktrees: PathBuf, runs: RunManager, notify: Notifier, program: Option<PathBuf>, probe: Option<Probe>) -> Self {
        Self::with_all(db_path, worktrees, runs, notify, program, probe, None)
    }

    pub fn with_all(
        db_path: PathBuf,
        worktrees: PathBuf,
        runs: RunManager,
        notify: Notifier,
        program: Option<PathBuf>,
        probe: Option<Probe>,
        secrets: Option<crate::mcp::secrets::Secrets>,
    ) -> Self {
        let probe = probe.unwrap_or_else(default_probe);
        let secrets = secrets.unwrap_or_else(|| Arc::new(crate::mcp::secrets::KeychainStore));
        Self { inner: Arc::new(Inner { db_path, worktrees, runs, notify, lock: Mutex::new(()), program, probe, secrets }) }
    }

    /// MCP servers for a run on behalf of this mission (profile ∪ squad ∪ mission).
    fn mcp_for(&self, c: &Connection, m: &missions::Mission, profile_id: Option<&str>, provider: &str, permission: &str) -> AppResult<Option<crate::mcp::repo::McpAttach>> {
        let dir = self.inner.db_path.parent().map(|p| p.join("mcp-run")).unwrap_or_else(std::env::temp_dir);
        let (attach, notes) = crate::mcp::repo::prepare_run(c, self.inner.secrets.as_ref(), &dir, provider, permission, profile_id, m.squad_id.as_deref(), Some(&m.id))?;
        for n in notes {
            tasks::event(c, &m.id, None, "maestro", &n)?;
        }
        Ok(attach)
    }

    fn conn(&self) -> AppResult<Connection> {
        open(&self.inner.db_path)
    }

    fn guard(&self) -> AppResult<std::sync::MutexGuard<'_, ()>> {
        self.inner.lock.lock().map_err(|_| AppError::Internal("mission lock poisoned".into()))
    }

    fn notify(&self, mission_id: &str) {
        (self.inner.notify)(mission_id);
    }

    fn workspace_root(conn: &Connection, m: &missions::Mission) -> AppResult<PathBuf> {
        let ws = m.workspace_id.as_deref().ok_or_else(|| AppError::Invalid("associe a missão a um workspace".into()))?;
        Ok(PathBuf::from(workspaces::get(conn, ws)?.path))
    }

    // ── Details ──────────────────────────────────────────────────────────

    pub fn detail(&self, mission_id: &str) -> AppResult<MissionDetail> {
        let c = self.conn()?;
        let mission = missions::get(&c, mission_id)?;
        let task_list = tasks::list(&c, mission_id)?;
        let events = tasks::events(&c, mission_id, 300)?;
        let (run_list, totals) = Self::totals(&c, mission_id)?;
        Ok(MissionDetail { mission, tasks: task_list, events, runs: run_list, totals })
    }

    fn totals(c: &Connection, mission_id: &str) -> AppResult<(Vec<Run>, Totals)> {
        let mut run_list = Vec::new();
        for id in tasks::run_ids(c, mission_id)? {
            if let Ok(r) = runs::get(c, &id) {
                run_list.push(r);
            }
        }
        let reported: Vec<f64> = run_list.iter().filter_map(|r| r.cost_usd).collect();
        let totals = Totals {
            runs: run_list.len() as i64,
            input_tokens: run_list.iter().filter_map(|r| r.input_tokens).sum(),
            output_tokens: run_list.iter().filter_map(|r| r.output_tokens).sum(),
            cost_usd: (!reported.is_empty()).then(|| reported.iter().sum()),
            cost_reported_runs: reported.len() as i64,
            duration_ms: run_list.iter().filter_map(|r| r.duration_ms).sum(),
        };
        Ok((run_list, totals))
    }

    // ── Maestro: limits ──────────────────────────────────────────────────

    /// Why the mission must not start more runs (run cap / reported-cost budget).
    fn limit_reason(c: &Connection, m: &missions::Mission) -> AppResult<Option<String>> {
        let n: i64 = c.query_row("SELECT count(*) FROM mission_runs WHERE mission_id = ?1", [&m.id], |r| r.get(0))?;
        if n >= m.max_runs {
            return Ok(Some(format!("limite de {} execuções de agente atingido", m.max_runs)));
        }
        if let Some(budget) = m.budget_usd {
            let spent: Option<f64> = c.query_row(
                "SELECT sum(a.cost_usd) FROM mission_runs r JOIN agent_runs a ON a.id = r.run_id WHERE r.mission_id = ?1",
                [&m.id],
                |r| r.get(0),
            )?;
            if let Some(s) = spent.filter(|s| *s >= budget) {
                return Ok(Some(format!("orçamento de US$ {budget:.2} atingido (US$ {s:.2} informados pelos CLIs)")));
            }
        }
        Ok(None)
    }

    fn pause_for_limit(c: &Connection, mission_id: &str, reason: &str) -> AppResult<()> {
        // Event first: observers that see "paused" always find the reason.
        tasks::event(c, mission_id, None, "blocked", &format!("Maestro pausou a missão: {reason}. Ajuste os limites e retome."))?;
        missions::set_status(c, mission_id, "paused")
    }

    // ── Planning ─────────────────────────────────────────────────────────

    pub fn plan(&self, mission_id: &str) -> AppResult<Run> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        if !matches!(m.status.as_str(), "draft" | "planned") {
            return Err(AppError::Invalid("o plano só pode ser gerado antes do início da missão".into()));
        }
        if m.planning {
            return Err(AppError::Invalid("o planejamento já está em andamento".into()));
        }
        let root = Self::workspace_root(&c, &m)?;
        // With a squad, planning is done by one of its members and restricted to its roles.
        let (all, squad_ctx) = Self::planning_profiles(&c, &m)?;
        let planner = all
            .iter()
            .filter(|p| ["planner", "architect", "pm"].contains(&p.role.as_str()))
            .min_by_key(|p| ["planner", "architect", "pm"].iter().position(|r| *r == p.role).unwrap_or(9))
            .or_else(|| all.iter().find(|p| p.id == "builtin:architect"))
            .or_else(|| all.first())
            .ok_or_else(|| AppError::Invalid("nenhum perfil de agente disponível".into()))?;
        let briefing = match &squad_ctx {
            Some(ctx) => format!("{}\n\n{ctx}", m.briefing),
            None => m.briefing.clone(),
        };
        let skills = Self::skills_for(&c, &m, Some(&planner.id))?;
        let req = RunRequest {
            workspace_id: m.workspace_id.clone().unwrap_or_default(),
            profile_id: Some(planner.id.clone()),
            parent_run_id: None,
            provider_id: planner.provider_id.clone(),
            model: planner.model.clone(),
            permission: "read".into(),
            instructions: crate::skills::repo::with_skills(Some(planner.instructions.clone()), &skills),
            skill_labels: skills.labels.clone(),
            mcp: self.mcp_for(&c, &m, Some(&planner.id), &planner.provider_id, "read")?,
            mission_id: Some(m.id.clone()),
            extra_skill_ids: vec![],
            extra_mcp_ids: vec![],
            max_turns: planner.max_turns,
            prompt: tasks::planner_prompt(&m.title, &briefing, &all),
            resume_session: None,
            use_worktree: false,
            confirm_full: false,
            cwd_override: None,
            cwd_branch: None,
        };
        let me = self.clone();
        let mid = mission_id.to_string();
        let run = self.inner.runs.start_with_program(
            &self.inner.db_path,
            &root,
            &self.inner.worktrees,
            req,
            self.inner.program.clone(),
            Some(Box::new(move |run| me.on_plan_finished(&mid, run))),
        )?;
        c.execute("UPDATE missions SET plan_run_id = ?2, planning = 1 WHERE id = ?1", rusqlite::params![mission_id, run.id])?;
        tasks::link_run(&c, mission_id, None, &run.id, "plan")?;
        tasks::event(&c, mission_id, None, "plan", &format!("Planejamento iniciado por {}", planner.name))?;
        drop(c);
        self.notify(mission_id);
        Ok(run)
    }

    /// Profiles the planner may assign (squad members, or every profile) plus squad guidance.
    fn planning_profiles(c: &Connection, m: &missions::Mission) -> AppResult<(Vec<profiles::Profile>, Option<String>)> {
        if let Some(sid) = &m.squad_id {
            if let Ok(squad) = crate::library::squads::get(c, sid) {
                let members = crate::library::squads::member_profiles(c, &squad)?;
                if !members.is_empty() {
                    let ctx = crate::library::squads::planning_context(&squad, &members);
                    return Ok((members, Some(ctx)));
                }
            }
        }
        Ok((profiles::list(c)?, None))
    }

    /// Enabled skills for a run on behalf of this mission (profile ∪ squad ∪ mission).
    fn skills_for(c: &Connection, m: &missions::Mission, profile_id: Option<&str>) -> AppResult<crate::skills::repo::Composed> {
        let composed = crate::skills::repo::compose(c, profile_id, m.squad_id.as_deref(), Some(&m.id))?;
        if !composed.dropped.is_empty() {
            tasks::event(c, &m.id, None, "maestro", &format!("Skills não aplicadas por excesso de tamanho: {}", composed.dropped.join(", ")))?;
        }
        Ok(composed)
    }

    fn squad_rules(c: &Connection, m: &missions::Mission) -> Option<String> {
        let squad = crate::library::squads::get(c, m.squad_id.as_deref()?).ok()?;
        let rules = squad.rules.trim();
        (!rules.is_empty()).then(|| format!("\n\nSQUAD \"{}\" RULES (follow them):\n{rules}", squad.name))
    }

    fn on_plan_finished(&self, mission_id: &str, run: Run) {
        let result = (|| -> AppResult<()> {
            let _g = self.guard()?;
            let c = self.conn()?;
            c.execute("UPDATE missions SET planning = 0 WHERE id = ?1", [mission_id])?;
            if run.status != "completed" {
                let why = run.error.clone().unwrap_or_else(|| run.status.clone());
                tasks::event(&c, mission_id, None, "error", &format!("O planejamento não foi concluído: {why}"))?;
                return Ok(());
            }
            let m = missions::get(&c, mission_id)?;
            let (allowed, _) = Self::planning_profiles(&c, &m)?;
            let all = profiles::list(&c)?;
            match tasks::parse_plan(run.result_text.as_deref().unwrap_or_default(), &allowed) {
                Ok(plan) => {
                    let created = tasks::replace(&c, mission_id, &plan, &all)?;
                    missions::set_status(&c, mission_id, "planned")?;
                    tasks::event(&c, mission_id, None, "plan", &format!("Plano gerado com {} tarefa(s) — revise e aprove para iniciar", created.len()))?;
                }
                Err(e) => tasks::event(&c, mission_id, None, "error", &format!("Plano inválido: {e}"))?,
            }
            Ok(())
        })();
        if let Err(e) = result {
            if let Ok(c) = self.conn() {
                let _ = tasks::event(&c, mission_id, None, "error", &format!("Erro ao processar o plano: {e}"));
            }
        }
        self.notify(mission_id);
    }

    pub fn save_tasks(&self, mission_id: &str, inputs: &[tasks::TaskInput]) -> AppResult<Vec<Task>> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        if !matches!(m.status.as_str(), "draft" | "planned") || m.planning {
            return Err(AppError::Invalid("as tarefas só podem ser editadas antes do início da missão".into()));
        }
        let all = profiles::list(&c)?;
        let saved = tasks::replace(&c, mission_id, inputs, &all)?;
        missions::set_status(&c, mission_id, "planned")?;
        tasks::event(&c, mission_id, None, "plan", &format!("Plano editado ({} tarefa(s))", saved.len()))?;
        drop(c);
        self.notify(mission_id);
        Ok(saved)
    }

    // ── Execution ────────────────────────────────────────────────────────

    pub fn start(&self, mission_id: &str) -> AppResult<()> {
        {
            let _g = self.guard()?;
            let c = self.conn()?;
            let m = missions::get(&c, mission_id)?;
            if m.status != "planned" {
                return Err(AppError::Invalid("aprove um plano antes de iniciar".into()));
            }
            if tasks::list(&c, mission_id)?.is_empty() {
                return Err(AppError::Invalid("a missão não tem tarefas".into()));
            }
            let root = Self::workspace_root(&c, &m)?;
            let base = git::head_commit(&root).ok_or_else(|| {
                AppError::Invalid("as tarefas rodam em worktrees isoladas: o workspace precisa ser um repositório Git com pelo menos um commit".into())
            })?;
            let branch = git::current_branch(&root);
            c.execute(
                "UPDATE missions SET status = 'running', base_commit = ?2, base_branch = ?3, approved_at = ?4, updated_at = ?4 WHERE id = ?1",
                rusqlite::params![mission_id, base, branch, crate::db::now_ms()],
            )?;
            tasks::event(&c, mission_id, None, "start", &format!("Plano aprovado — missão iniciada a partir de {}", &base[..7]))?;
        }
        self.advance(mission_id)
    }

    /// Launches every task whose dependencies are done, up to `max_parallel`,
    /// and completes the mission when all tasks are approved or skipped.
    pub fn advance(&self, mission_id: &str) -> AppResult<()> {
        let _g = self.guard()?;
        self.advance_locked(mission_id)?;
        self.notify(mission_id);
        Ok(())
    }

    fn advance_locked(&self, mission_id: &str) -> AppResult<()> {
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        if m.status != "running" {
            return Ok(());
        }
        let list = tasks::list(&c, mission_id)?;
        let done = |id: &String| list.iter().any(|t| &t.id == id && matches!(t.status.as_str(), "approved" | "skipped"));
        let mut running = list.iter().filter(|t| t.status == "running").count() as i64;
        for t in list.iter().filter(|t| t.status == "pending") {
            if running >= m.max_parallel {
                break;
            }
            if t.depends_on.iter().all(done) {
                if let Some(reason) = Self::limit_reason(&c, &m)? {
                    Self::pause_for_limit(&c, mission_id, &reason)?;
                    return Ok(());
                }
                match self.launch(&c, &m, t, &list) {
                    Ok(()) => running += 1,
                    Err(e) => {
                        tasks::set_status(&c, &t.id, "failed")?;
                        tasks::event(&c, mission_id, Some(&t.id), "error", &format!("“{}” não pôde iniciar: {e}", t.title))?;
                    }
                }
            }
        }
        let list = tasks::list(&c, mission_id)?;
        if !list.is_empty() && list.iter().all(|t| matches!(t.status.as_str(), "approved" | "skipped")) {
            // Report first, so "completed" is never observed without it.
            if let Err(e) = self.write_report(&c, mission_id) {
                tasks::event(&c, mission_id, None, "error", &format!("Não foi possível gerar o relatório: {e}"))?;
            }
            missions::set_status(&c, mission_id, "completed")?;
            tasks::event(&c, mission_id, None, "complete", "Todas as tarefas aprovadas — missão pronta para integração")?;
        } else if !list.iter().any(|t| t.status == "running") && list.iter().any(|t| matches!(t.status.as_str(), "failed" | "rejected" | "cancelled")) {
            let blocked = list.iter().filter(|t| t.status == "pending").count();
            if blocked > 0 || !list.iter().any(|t| t.status == "review") {
                tasks::event(&c, mission_id, None, "blocked", "Missão aguardando decisão: há tarefas com falha ou rejeitadas (tente novamente ou pule)")?;
            }
        }
        Ok(())
    }

    fn launch(&self, c: &Connection, m: &missions::Mission, t: &Task, all: &[Task]) -> AppResult<()> {
        let root = Self::workspace_root(c, m)?;
        let profile = profiles::get(c, t.profile_id.as_deref().unwrap_or_default())?;
        let deps: Vec<Task> = all.iter().filter(|x| t.depends_on.contains(&x.id) && x.status == "approved").cloned().collect();

        // Worktree: base = first dependency branch (or the mission base), then merge the other dependencies.
        let branch = format!("forge/mission-{}/{}-{}", &m.id[..8], slug(&t.key), t.attempts + 1);
        let base = deps.first().and_then(|d| d.worktree_branch.clone()).or_else(|| m.base_commit.clone());
        let wt = git::worktree_add(&root, &self.inner.worktrees, &branch, true, base.as_deref())?;
        let wt_path = PathBuf::from(&wt.path);
        for d in deps.iter().skip(1) {
            if let Some(b) = &d.worktree_branch {
                if let Err(e) = git::merge(&wt_path, b) {
                    let _ = git::merge_abort(&wt_path);
                    return Err(AppError::Invalid(format!("conflito ao combinar a dependência “{}”: {e}", d.title)));
                }
            }
        }
        let permission = profile.permission.clone();
        tasks::set_base_commit(c, &t.id, git::head_commit(&wt_path).as_deref())?;
        tasks::set_review(c, &t.id, None, None, None)?;
        // Maestro reassignment overrides the provider (the profile's model may not apply to it).
        let provider_id = t.provider_override.clone().unwrap_or_else(|| profile.provider_id.clone());
        let mut model = if provider_id == profile.provider_id { profile.model.clone() } else { None };
        // Harness (M16): pick the Claude tier by task size when the profile has no fixed model.
        if m.harness && model.is_none() && provider_id == "claude" {
            let cls = crate::harness::classify(&format!("{}\n{}\n{}", t.title, t.description, t.acceptance));
            let tier = crate::harness::claude_tier(&cls, crate::harness::Preference::Balanced);
            tasks::event(c, &m.id, Some(&t.id), "maestro", &format!("Harness: modelo {tier} para “{}” (tarefa {} de porte {})", t.title, cls.activity, cls.complexity))?;
            model = Some(tier.to_string());
        }
        let skills = Self::skills_for(c, m, Some(&profile.id))?;
        let req = RunRequest {
            workspace_id: m.workspace_id.clone().unwrap_or_default(),
            profile_id: Some(profile.id.clone()),
            parent_run_id: None,
            provider_id: provider_id.clone(),
            model,
            permission: if permission == "full" { "edit".into() } else { permission.clone() },
            instructions: crate::skills::repo::with_skills(Some(profile.instructions.clone()), &skills),
            skill_labels: skills.labels.clone(),
            mcp: self.mcp_for(c, m, Some(&profile.id), &provider_id, if permission == "full" { "edit" } else { permission.as_str() })?,
            mission_id: Some(m.id.clone()),
            extra_skill_ids: vec![],
            extra_mcp_ids: vec![],
            max_turns: profile.max_turns,
            prompt: format!("{}{}", tasks::task_prompt(&m.title, &m.briefing, t, &deps, permission != "read"), Self::squad_rules(c, m).unwrap_or_default()),
            resume_session: None,
            use_worktree: false,
            confirm_full: false,
            cwd_override: Some(wt_path),
            cwd_branch: Some(branch.clone()),
        };
        let run = self.start_task_run(m, t, req)?;
        tasks::set_run(c, &t.id, &run.id, &wt.path, &branch)?;
        tasks::link_run(c, &m.id, Some(&t.id), &run.id, "task")?;
        let via = providers::def(&provider_id).map(|d| d.name).unwrap_or("?");
        tasks::event(c, &m.id, Some(&t.id), "task", &format!("“{}” iniciada por {} ({via}) em {branch}", t.title, profile.name))?;
        Ok(())
    }

    fn start_task_run(&self, m: &missions::Mission, t: &Task, req: RunRequest) -> AppResult<Run> {
        let root = PathBuf::from(&req.cwd_override.clone().unwrap_or_default());
        let me = self.clone();
        let (mid, tid) = (m.id.clone(), t.id.clone());
        self.inner.runs.start_with_program(
            &self.inner.db_path,
            &root,
            &self.inner.worktrees,
            req,
            self.inner.program.clone(),
            Some(Box::new(move |run| me.on_task_finished(&mid, &tid, run))),
        )
    }

    fn on_task_finished(&self, mission_id: &str, task_id: &str, run: Run) {
        let result = (|| -> AppResult<()> {
            let _g = self.guard()?;
            let c = self.conn()?;
            let m = missions::get(&c, mission_id)?;
            let t = tasks::get(&c, task_id)?;
            if t.run_id.as_deref() != Some(run.id.as_str()) {
                return Ok(()); // stale run (task was retried meanwhile)
            }
            match run.status.as_str() {
                "completed" => {
                    let commit = match &t.worktree_path {
                        Some(p) if Path::new(p).is_dir() => git::commit_all_if_dirty(Path::new(p), &format!("forge: {}\n\nMission: {}\nTask: {}", t.title, m.title, t.key)),
                        _ => Ok(None),
                    };
                    match commit {
                        Ok(sha) if m.mode != "free" => {
                            tasks::set_status(&c, task_id, "review")?;
                            let what = sha.map(|s| format!("commit {}", &s[..7])).unwrap_or_else(|| "sem alterações de arquivos".into());
                            tasks::event(&c, mission_id, Some(task_id), "task", &format!("“{}” concluída ({what})", t.title))?;
                            let t2 = tasks::get(&c, task_id)?;
                            if let Err(e) = self.launch_review(&c, &m, &t2) {
                                tasks::event(&c, mission_id, Some(task_id), "blocked", &format!("Maestro não conseguiu iniciar a revisão de “{}”: {e} — revise manualmente", t.title))?;
                            }
                        }
                        Ok(sha) => {
                            let next = if m.require_review { "review" } else { "approved" };
                            tasks::set_status(&c, task_id, next)?;
                            let what = sha.map(|s| format!("commit {}", &s[..7])).unwrap_or_else(|| "sem alterações de arquivos".into());
                            let msg = if m.require_review {
                                format!("“{}” concluída ({what}) — aguardando revisão", t.title)
                            } else {
                                format!("“{}” concluída ({what})", t.title)
                            };
                            tasks::event(&c, mission_id, Some(task_id), "task", &msg)?;
                        }
                        Err(e) => {
                            tasks::set_status(&c, task_id, "failed")?;
                            tasks::event(&c, mission_id, Some(task_id), "error", &format!("“{}”: falha ao registrar o commit: {e}", t.title))?;
                        }
                    }
                }
                "cancelled" => {
                    tasks::set_status(&c, task_id, "cancelled")?;
                    tasks::event(&c, mission_id, Some(task_id), "task", &format!("“{}” cancelada", t.title))?;
                }
                _ => {
                    tasks::set_status(&c, task_id, "failed")?;
                    let why = run.error.clone().unwrap_or_else(|| "erro desconhecido".into());
                    tasks::event(&c, mission_id, Some(task_id), "error", &format!("“{}” falhou: {why}", t.title))?;
                    if m.mode == "agentic" {
                        let t2 = tasks::get(&c, task_id)?;
                        self.handle_failure(&c, &m, &t2, &format!("The previous attempt failed with: {why}"))?;
                    }
                }
            }
            drop(c);
            self.advance_locked(mission_id)
        })();
        if let Err(e) = result {
            if let Ok(c) = self.conn() {
                let _ = tasks::event(&c, mission_id, Some(task_id), "error", &format!("Erro interno: {e}"));
            }
        }
        self.notify(mission_id);
    }

    // ── Review & task actions ────────────────────────────────────────────

    fn task_in(&self, c: &Connection, mission_id: &str, task_id: &str) -> AppResult<Task> {
        let t = tasks::get(c, task_id)?;
        if t.mission_id != mission_id {
            return Err(AppError::Invalid("a tarefa não pertence a esta missão".into()));
        }
        Ok(t)
    }

    pub fn approve_task(&self, mission_id: &str, task_id: &str) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let t = self.task_in(&c, mission_id, task_id)?;
        if t.status != "review" {
            return Err(AppError::Invalid("só tarefas em revisão podem ser aprovadas".into()));
        }
        tasks::set_status(&c, task_id, "approved")?;
        tasks::event(&c, mission_id, Some(task_id), "review", &format!("“{}” aprovada", t.title))?;
        self.advance_locked(mission_id)?;
        self.notify(mission_id);
        Ok(())
    }

    pub fn reject_task(&self, mission_id: &str, task_id: &str, feedback: &str) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let t = self.task_in(&c, mission_id, task_id)?;
        if t.status != "review" {
            return Err(AppError::Invalid("só tarefas em revisão podem ser rejeitadas".into()));
        }
        tasks::set_status(&c, task_id, "rejected")?;
        tasks::set_feedback(&c, task_id, Some(feedback.trim()).filter(|f| !f.is_empty()))?;
        tasks::event(&c, mission_id, Some(task_id), "review", &format!("“{}” rejeitada{}", t.title, if feedback.trim().is_empty() { String::new() } else { format!(": {}", feedback.trim()) }))?;
        self.notify(mission_id);
        Ok(())
    }

    /// Runs the task again in a fresh worktree (with the reviewer's feedback, if any).
    pub fn retry_task(&self, mission_id: &str, task_id: &str, feedback: Option<&str>) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        let t = self.task_in(&c, mission_id, task_id)?;
        if !matches!(t.status.as_str(), "rejected" | "failed" | "cancelled") {
            return Err(AppError::Invalid("só tarefas rejeitadas, com falha ou canceladas podem ser refeitas".into()));
        }
        if !matches!(m.status.as_str(), "running" | "paused") {
            return Err(AppError::Invalid("a missão não está em execução".into()));
        }
        if let Some(f) = feedback.map(str::trim).filter(|f| !f.is_empty()) {
            tasks::set_feedback(&c, task_id, Some(f))?;
        }
        self.retry_internal(&c, &m, &t)?;
        tasks::event(&c, mission_id, Some(task_id), "task", &format!("“{}” será refeita", t.title))?;
        self.advance_locked(mission_id)?;
        self.notify(mission_id);
        Ok(())
    }

    pub fn skip_task(&self, mission_id: &str, task_id: &str) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let t = self.task_in(&c, mission_id, task_id)?;
        if !matches!(t.status.as_str(), "pending" | "rejected" | "failed" | "cancelled") {
            return Err(AppError::Invalid("esta tarefa não pode ser pulada agora".into()));
        }
        tasks::set_status(&c, task_id, "skipped")?;
        tasks::event(&c, mission_id, Some(task_id), "task", &format!("“{}” pulada", t.title))?;
        self.advance_locked(mission_id)?;
        self.notify(mission_id);
        Ok(())
    }

    pub fn cancel_task(&self, mission_id: &str, task_id: &str) -> AppResult<()> {
        let c = self.conn()?;
        let t = self.task_in(&c, mission_id, task_id)?;
        if t.status != "running" {
            return Err(AppError::Invalid("a tarefa não está em execução".into()));
        }
        self.inner.runs.cancel(t.run_id.as_deref().unwrap_or_default())
    }

    // ── Mission controls ─────────────────────────────────────────────────

    pub fn pause(&self, mission_id: &str) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        if missions::get(&c, mission_id)?.status != "running" {
            return Err(AppError::Invalid("a missão não está em execução".into()));
        }
        missions::set_status(&c, mission_id, "paused")?;
        tasks::event(&c, mission_id, None, "pause", "Missão pausada — tarefas em andamento terminam, nenhuma nova é iniciada")?;
        self.notify(mission_id);
        Ok(())
    }

    pub fn resume(&self, mission_id: &str) -> AppResult<()> {
        let _g = self.guard()?;
        let c = self.conn()?;
        if missions::get(&c, mission_id)?.status != "paused" {
            return Err(AppError::Invalid("a missão não está pausada".into()));
        }
        missions::set_status(&c, mission_id, "running")?;
        tasks::event(&c, mission_id, None, "resume", "Missão retomada")?;
        self.advance_locked(mission_id)?;
        self.notify(mission_id);
        Ok(())
    }

    pub fn cancel(&self, mission_id: &str) -> AppResult<()> {
        let running: Vec<String>;
        {
            let _g = self.guard()?;
            let c = self.conn()?;
            let m = missions::get(&c, mission_id)?;
            if matches!(m.status.as_str(), "completed" | "cancelled") {
                return Err(AppError::Invalid(format!("a missão já está {}", m.status)));
            }
            missions::set_status(&c, mission_id, "cancelled")?;
            let list = tasks::list(&c, mission_id)?;
            for t in list.iter().filter(|t| t.status == "pending") {
                tasks::set_status(&c, &t.id, "cancelled")?;
            }
            running = list.iter().filter(|t| t.status == "running").filter_map(|t| t.run_id.clone()).collect();
            tasks::event(&c, mission_id, None, "cancel", "Missão cancelada")?;
        }
        for r in running {
            let _ = self.inner.runs.cancel(&r);
        }
        self.notify(mission_id);
        Ok(())
    }

    /// Merges approved task branches into the workspace's current branch, in plan order.
    pub fn integrate(&self, mission_id: &str) -> AppResult<usize> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        if m.status != "completed" {
            return Err(AppError::Invalid("a missão precisa estar concluída para integrar".into()));
        }
        let root = Self::workspace_root(&c, &m)?;
        let st = git::status(&root)?;
        if st.merging {
            return Err(AppError::Invalid("há um merge em andamento no workspace: conclua ou aborte no Git antes de integrar".into()));
        }
        if st.files.iter().any(|f| !f.untracked) {
            return Err(AppError::Invalid("o workspace tem alterações não commitadas: faça commit ou stash antes de integrar".into()));
        }
        let mut merged = 0;
        for t in tasks::list(&c, mission_id)? {
            if t.status != "approved" || t.integrated_at.is_some() {
                continue;
            }
            let Some(branch) = &t.worktree_branch else { continue };
            match git::merge(&root, branch) {
                Ok(_) => {
                    tasks::mark_integrated(&c, &t.id)?;
                    merged += 1;
                }
                Err(e) => {
                    tasks::event(&c, mission_id, Some(&t.id), "error", &format!("Conflito ao integrar “{}”: resolva no Git, conclua o merge e clique em Integrar novamente. ({e})", t.title))?;
                    drop(c);
                    self.notify(mission_id);
                    return Err(AppError::Conflict(format!("conflito ao integrar “{}” — resolva no Git e integre novamente", t.title)));
                }
            }
        }
        let pending = tasks::list(&c, mission_id)?.iter().any(|t| t.status == "approved" && t.integrated_at.is_none() && t.worktree_branch.is_some());
        if !pending {
            c.execute("UPDATE missions SET integrated_at = ?2 WHERE id = ?1", rusqlite::params![mission_id, crate::db::now_ms()])?;
        }
        tasks::event(&c, mission_id, None, "integrate", &format!("{merged} branch(es) integradas em {}", m.base_branch.as_deref().unwrap_or("HEAD")))?;
        let _ = self.write_report(&c, mission_id);
        drop(c);
        self.notify(mission_id);
        Ok(merged)
    }

    // ── Maestro: review, failure handling, report ────────────────────────

    fn retry_internal(&self, c: &Connection, m: &missions::Mission, t: &Task) -> AppResult<()> {
        self.discard_worktree(c, m, t);
        tasks::set_status(c, &t.id, "pending")
    }

    /// Starts the Code Reviewer (read-only) inside the task's worktree.
    fn launch_review(&self, c: &Connection, m: &missions::Mission, t: &Task) -> AppResult<()> {
        if let Some(reason) = Self::limit_reason(c, m)? {
            Self::pause_for_limit(c, &m.id, &reason)?;
            return Err(AppError::Invalid(reason));
        }
        let all = profiles::list(c)?;
        let reviewer = all
            .iter()
            .find(|p| p.id == "builtin:reviewer")
            .or_else(|| all.iter().find(|p| p.permission == "read"))
            .ok_or_else(|| AppError::Invalid("nenhum perfil revisor disponível".into()))?;
        let wt = t.worktree_path.clone().ok_or_else(|| AppError::Invalid("a tarefa não tem worktree".into()))?;
        let skills = Self::skills_for(c, m, Some(&reviewer.id))?;
        let req = RunRequest {
            workspace_id: m.workspace_id.clone().unwrap_or_default(),
            profile_id: Some(reviewer.id.clone()),
            parent_run_id: None,
            provider_id: reviewer.provider_id.clone(),
            model: reviewer.model.clone(),
            permission: "read".into(),
            instructions: crate::skills::repo::with_skills(Some(reviewer.instructions.clone()), &skills),
            skill_labels: skills.labels.clone(),
            mcp: self.mcp_for(c, m, Some(&reviewer.id), &reviewer.provider_id, "read")?,
            mission_id: Some(m.id.clone()),
            extra_skill_ids: vec![],
            extra_mcp_ids: vec![],
            max_turns: reviewer.max_turns,
            prompt: maestro::review_prompt(&m.title, t, t.base_commit.as_deref()),
            resume_session: None,
            use_worktree: false,
            confirm_full: false,
            cwd_override: Some(PathBuf::from(&wt)),
            cwd_branch: t.worktree_branch.clone(),
        };
        let me = self.clone();
        let (mid, tid) = (m.id.clone(), t.id.clone());
        let run = self.inner.runs.start_with_program(
            &self.inner.db_path,
            Path::new(&wt),
            &self.inner.worktrees,
            req,
            self.inner.program.clone(),
            Some(Box::new(move |run| me.on_review_finished(&mid, &tid, run))),
        )?;
        tasks::set_review(c, &t.id, Some(&run.id), None, None)?;
        tasks::link_run(c, &m.id, Some(&t.id), &run.id, "review")?;
        tasks::event(c, &m.id, Some(&t.id), "maestro", &format!("Maestro: {} revisando “{}”", reviewer.name, t.title))?;
        Ok(())
    }

    fn on_review_finished(&self, mission_id: &str, task_id: &str, run: Run) {
        let result = (|| -> AppResult<()> {
            let _g = self.guard()?;
            let c = self.conn()?;
            let m = missions::get(&c, mission_id)?;
            let t = tasks::get(&c, task_id)?;
            if t.review_run_id.as_deref() != Some(run.id.as_str()) || t.status != "review" {
                return Ok(()); // superseded, or the human already decided
            }
            let review = if run.status == "completed" { maestro::parse_review(run.result_text.as_deref().unwrap_or_default()) } else { None };
            match review {
                Some(r) => {
                    let feedback = r.as_feedback();
                    tasks::set_review(&c, task_id, None, Some(if r.approve { "approve" } else { "reject" }), Some(&feedback))?;
                    let verdict = if r.approve { "aprovar" } else { "rejeitar" };
                    tasks::event(&c, mission_id, Some(task_id), "maestro", &format!("Maestro recomenda {verdict} “{}”: {}", t.title, r.summary))?;
                    if m.mode == "agentic" {
                        if r.approve {
                            tasks::set_status(&c, task_id, "approved")?;
                            tasks::event(&c, mission_id, Some(task_id), "review", &format!("“{}” aprovada automaticamente pelo Maestro", t.title))?;
                        } else {
                            tasks::set_status(&c, task_id, "rejected")?;
                            let t2 = tasks::get(&c, task_id)?;
                            self.handle_failure(&c, &m, &t2, &feedback)?;
                        }
                    }
                }
                None => {
                    let why = run.error.clone().unwrap_or_else(|| "veredito ausente ou inválido".into());
                    tasks::set_review(&c, task_id, None, None, Some(&format!("Revisão automática indisponível: {why}")))?;
                    tasks::event(&c, mission_id, Some(task_id), "blocked", &format!("Maestro não conseguiu revisar “{}” ({why}) — revise manualmente", t.title))?;
                }
            }
            drop(c);
            self.advance_locked(mission_id)
        })();
        if let Err(e) = result {
            if let Ok(c) = self.conn() {
                let _ = tasks::event(&c, mission_id, Some(task_id), "error", &format!("Erro interno na revisão: {e}"));
            }
        }
        self.notify(mission_id);
    }

    /// Agentic policy: retry → reassign to another provider → ask the human.
    fn handle_failure(&self, c: &Connection, m: &missions::Mission, t: &Task, feedback: &str) -> AppResult<()> {
        let current = match &t.provider_override {
            Some(p) => p.clone(),
            None => profiles::get(c, t.profile_id.as_deref().unwrap_or_default()).map(|p| p.provider_id).unwrap_or_default(),
        };
        let candidate = if t.attempts >= m.max_attempts && t.provider_override.is_none() { (self.inner.probe)(&current) } else { None };
        match maestro::decide_on_failure(t.attempts, m.max_attempts, t.provider_override.is_some(), candidate) {
            FailureAction::Retry => {
                tasks::set_feedback(c, &t.id, Some(feedback))?;
                self.retry_internal(c, m, t)?;
                tasks::event(c, &m.id, Some(&t.id), "maestro", &format!("Maestro: refazendo “{}” (tentativa {} de {})", t.title, t.attempts + 1, m.max_attempts))?;
            }
            FailureAction::Reassign(p) => {
                tasks::set_feedback(c, &t.id, Some(feedback))?;
                tasks::set_provider_override(c, &t.id, Some(&p))?;
                self.retry_internal(c, m, t)?;
                let name = providers::def(&p).map(|d| d.name).unwrap_or("outro provider");
                tasks::event(c, &m.id, Some(&t.id), "maestro", &format!("Maestro: “{}” falhou {} vez(es) — reatribuída para {name}", t.title, t.attempts))?;
            }
            FailureAction::Escalate => {
                tasks::event(c, &m.id, Some(&t.id), "blocked", &format!("Maestro: “{}” precisa da sua intervenção após {} tentativa(s) — refaça com orientações ou pule", t.title, t.attempts))?;
            }
        }
        Ok(())
    }

    fn write_report(&self, c: &Connection, mission_id: &str) -> AppResult<()> {
        let m = missions::get(c, mission_id)?;
        let root = Self::workspace_root(c, &m).ok();
        let list = tasks::list(c, mission_id)?;
        let all = profiles::list(c)?;
        let items: Vec<maestro::ReportTask> = list
            .iter()
            .map(|t| {
                let profile = all.iter().find(|p| Some(&p.id) == t.profile_id.as_ref());
                let provider = t.provider_override.clone().or_else(|| profile.map(|p| p.provider_id.clone())).unwrap_or_default();
                let commits = match (&root, &t.base_commit, &t.worktree_branch) {
                    (Some(r), Some(b), Some(br)) => git::commits_between(r, b, br).unwrap_or_default(),
                    _ => vec![],
                };
                maestro::ReportTask {
                    task: t,
                    profile: profile.map(|p| p.name.clone()).unwrap_or_else(|| "—".into()),
                    provider: providers::def(&provider).map(|d| d.name.to_string()).unwrap_or(provider),
                    commits,
                }
            })
            .collect();
        let (_, tot) = Self::totals(c, mission_id)?;
        let report = maestro::build_report(&m, &items, tot.runs, tot.input_tokens, tot.output_tokens, tot.cost_usd, tot.cost_reported_runs);
        missions::set_report(c, mission_id, &report)?;
        tasks::event(c, mission_id, None, "maestro", "Relatório final atualizado")
    }

    fn discard_worktree(&self, c: &Connection, m: &missions::Mission, t: &Task) {
        if let (Some(path), Ok(root)) = (&t.worktree_path, Self::workspace_root(c, m)) {
            if Path::new(path).exists() {
                let canonical = std::fs::canonicalize(path).map(|p| p.display().to_string()).unwrap_or_else(|_| path.clone());
                let _ = git::worktree_remove(&root, &canonical, true).or_else(|_| git::worktree_remove(&root, path, true));
            }
            let _ = tasks::clear_worktree(c, &t.id);
        }
    }

    /// Removes the worktrees of finished tasks (their branches are kept).
    pub fn cleanup(&self, mission_id: &str) -> AppResult<usize> {
        let _g = self.guard()?;
        let c = self.conn()?;
        let m = missions::get(&c, mission_id)?;
        let mut n = 0;
        for t in tasks::list(&c, mission_id)? {
            if t.status == "running" || t.worktree_path.is_none() {
                continue;
            }
            self.discard_worktree(&c, &m, &t);
            n += 1;
        }
        tasks::event(&c, mission_id, None, "cleanup", &format!("{n} worktree(s) removida(s); as branches foram mantidas"))?;
        drop(c);
        self.notify(mission_id);
        Ok(n)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A fake agent CLI: answers the planner with a 2-task plan; for tasks it
    /// writes a file named after the task into the working directory.
    fn fake_cli(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-agent.sh");
        let plan = r#"{"type":"result","subtype":"success","is_error":false,"result":"<forge-plan>{\"tasks\":[{\"key\":\"api\",\"title\":\"Create API\",\"profile\":\"backend\",\"depends_on\":[],\"description\":\"write api.txt\"},{\"key\":\"ui\",\"title\":\"Create UI\",\"profile\":\"frontend\",\"depends_on\":[\"api\"],\"description\":\"write ui.txt\"}]}</forge-plan>","duration_ms":5,"num_turns":1,"usage":{"input_tokens":3,"output_tokens":4}}"#;
        let script = format!(
            r#"#!/bin/sh
ALL="$* $(cat 2>/dev/null)"
prev=""; for a in "$@"; do if [ "$prev" = "--model" ]; then echo "$a" >> models.txt; fi; if [ "$prev" = "--mcp-config" ]; then cat "$a" > mcp-seen.json; ls -l "$a" | cut -c1-10 > mcp-perm.txt; echo "$a" > mcp-path.txt; fi; prev="$a"; done
case "$ALL" in *'<skill name="testing-expert"'*) echo seen > skill-seen.txt ;; esac
case "$ALL" in *'<skill name="react-expert"'*) echo leaked > disabled-skill-leaked.txt ;; esac
case "$ALL" in
  *forge-review*) if test -f fixed.txt; then V=approve; else V=reject; fi; echo "{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"review <forge-review>{{\\\"verdict\\\":\\\"$V\\\",\\\"summary\\\":\\\"needs fixed.txt\\\",\\\"issues\\\":[\\\"create fixed.txt\\\"]}}</forge-review>\",\"duration_ms\":5,\"num_turns\":1}}" ;;
  *forge-plan*) echo '{{"type":"system","subtype":"init","session_id":"plan-s","model":"fake"}}'; echo '{plan}' ;;
  *"YOUR TASK: Create API"*) echo api > api.txt; case "$ALL" in *"REVIEWER FEEDBACK"*) echo fixed > fixed.txt ;; esac; test -f ui.txt && echo "ui exists too early" > error.txt; echo '{{"type":"system","subtype":"init","session_id":"api-s","model":"fake"}}'; echo '{{"type":"result","subtype":"success","is_error":false,"result":"api done","duration_ms":5,"num_turns":1,"usage":{{"input_tokens":10,"output_tokens":2}},"total_cost_usd":0.01}}' ;;
  *"YOUR TASK: Create UI"*) test -f api.txt || exit 7; echo ui > ui.txt; echo fixed > fixed.txt; echo '{{"type":"result","subtype":"success","is_error":false,"result":"ui done","duration_ms":5,"num_turns":1,"usage":{{"input_tokens":20,"output_tokens":3}}}}' ;;
  *) exit 9 ;;
esac
"#
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn sh(dir: &Path, args: &[&str]) {
        let out = git::run(dir, args, None, Duration::from_secs(30)).unwrap();
        assert!(out.success, "git {args:?}: {}", out.stderr);
    }

    fn wait(engine: &MissionEngine, mid: &str, what: &str, f: impl Fn(&MissionDetail) -> bool) -> MissionDetail {
        let start = Instant::now();
        loop {
            let d = engine.detail(mid).unwrap();
            if f(&d) {
                return d;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "timeout waiting for {what}: {:?} {:?}", d.mission.status, d.tasks.iter().map(|t| (&t.key, &t.status)).collect::<Vec<_>>());
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn full_mission_plan_execute_review_integrate() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("README.md"), "x").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "init"]);
        let repo = std::fs::canonicalize(&repo).unwrap();

        let db_path = tmp.path().join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        let mid = db
            .with(|c| {
                profiles::ensure_builtins(c, "claude")?;
                let ws = workspaces::add(c, &repo.display().to_string(), "repo")?;
                let m = missions::create(c, &missions::NewMission { workspace_id: Some(ws.id), title: "App".into(), briefing: "Build it".into(), mode: "free".into() })?;
                Ok(m.id)
            })
            .unwrap();
        drop(db);

        let notified = Arc::new(Mutex::new(0usize));
        let n2 = notified.clone();
        let engine = MissionEngine::with_program(
            db_path,
            tmp.path().join("wt"),
            RunManager::default(),
            Arc::new(move |_| *n2.lock().unwrap() += 1),
            Some(fake_cli(tmp.path())),
        );

        // Start requires a plan.
        assert!(engine.start(&mid).is_err());
        engine.plan(&mid).unwrap();
        assert!(engine.plan(&mid).is_err(), "no double planning");
        let d = wait(&engine, &mid, "plan", |d| d.mission.status == "planned");
        assert_eq!(d.tasks.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(), ["api", "ui"]);
        assert_eq!(d.tasks[1].depends_on, vec![d.tasks[0].id.clone()]);

        engine.start(&mid).unwrap();
        // Only "api" runs first (ui depends on it).
        let d = wait(&engine, &mid, "api review", |d| d.tasks[0].status == "review");
        assert_eq!(d.tasks[1].status, "pending");
        let api_wt = PathBuf::from(d.tasks[0].worktree_path.clone().unwrap());
        assert!(api_wt.join("api.txt").is_file());
        assert!(!repo.join("api.txt").exists(), "the user's checkout is untouched");
        assert!(git::status(&api_wt).unwrap().files.is_empty(), "task result was committed");

        // Reject → retry with feedback → review again.
        engine.reject_task(&mid, &d.tasks[0].id, "add more").unwrap();
        engine.retry_task(&mid, &d.tasks[0].id, None).unwrap();
        let d = wait(&engine, &mid, "api review 2", |d| d.tasks[0].status == "review" && d.tasks[0].attempts == 2);
        assert_eq!(d.tasks[0].feedback.as_deref(), Some("add more"));
        assert!(!api_wt.exists(), "old attempt worktree discarded");

        engine.approve_task(&mid, &d.tasks[0].id).unwrap();
        // "ui" starts from api's branch, so api.txt is present there.
        let d = wait(&engine, &mid, "ui review", |d| d.tasks[1].status == "review");
        let ui_wt = PathBuf::from(d.tasks[1].worktree_path.clone().unwrap());
        assert!(ui_wt.join("api.txt").is_file() && ui_wt.join("ui.txt").is_file());
        engine.approve_task(&mid, &d.tasks[1].id).unwrap();
        let d = wait(&engine, &mid, "completed", |d| d.mission.status == "completed");
        assert!(d.totals.runs >= 4, "plan + 3 task runs");
        assert_eq!(d.totals.cost_reported_runs, 2, "only runs that reported cost are summed");

        // Integration refuses a dirty workspace, then merges both branches.
        std::fs::write(repo.join("README.md"), "dirty").unwrap();
        assert!(engine.integrate(&mid).is_err());
        sh(&repo, &["checkout", "--", "README.md"]);
        assert_eq!(engine.integrate(&mid).unwrap(), 2);
        assert!(repo.join("api.txt").is_file() && repo.join("ui.txt").is_file());
        assert!(engine.detail(&mid).unwrap().mission.integrated_at.is_some());
        assert_eq!(engine.integrate(&mid).unwrap(), 0, "idempotent");

        assert_eq!(engine.cleanup(&mid).unwrap(), 2);
        assert!(!ui_wt.exists());
        assert!(*notified.lock().unwrap() > 5);
    }

    #[test]
    fn failing_task_blocks_until_skipped_and_cancel_stops_everything() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("a"), "x").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "init"]);
        let db_path = tmp.path().join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        let mid = db
            .with(|c| {
                profiles::ensure_builtins(c, "claude")?;
                let ws = workspaces::add(c, &std::fs::canonicalize(&repo).unwrap().display().to_string(), "r")?;
                let m = missions::create(c, &missions::NewMission { workspace_id: Some(ws.id), title: "M".into(), briefing: "B".into(), mode: "free".into() })?;
                Ok(m.id)
            })
            .unwrap();
        drop(db);
        let engine = MissionEngine::with_program(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())));
        // A manual plan whose first task makes the fake CLI exit 9 (unknown task).
        let input = |k: &str, title: &str, deps: Vec<String>| tasks::TaskInput {
            key: k.into(),
            title: title.into(),
            description: "x".into(),
            acceptance: String::new(),
            profile_id: Some("builtin:backend".into()),
            depends_on: deps,
        };
        engine.save_tasks(&mid, &[input("bad", "Unknown work", vec![]), input("ui", "Create UI", vec!["bad".into()])]).unwrap();
        engine.start(&mid).unwrap();
        // The "blocked" event is written right after the status change, in the same engine pass.
        let d = wait(&engine, &mid, "blocked", |d| d.tasks[0].status == "failed" && d.events.iter().any(|e| e.kind == "blocked"));
        assert_eq!(d.tasks[1].status, "pending", "dependents wait");
        // Skipping the failed task unblocks the dependent (which then fails: api.txt is missing → exit 7).
        engine.skip_task(&mid, &d.tasks[0].id).unwrap();
        wait(&engine, &mid, "ui failed", |d| d.tasks[1].status == "failed");
        engine.cancel(&mid).unwrap();
        assert_eq!(engine.detail(&mid).unwrap().mission.status, "cancelled");
        assert!(engine.cancel(&mid).is_err());
    }

    fn setup_repo_and_mission(tmp: &Path, mode: &str) -> (PathBuf, PathBuf, String) {
        let repo = tmp.join("repo");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("README.md"), "x").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "init"]);
        let repo = std::fs::canonicalize(&repo).unwrap();
        let db_path = tmp.join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        let mid = db
            .with(|c| {
                profiles::ensure_builtins(c, "claude")?;
                let ws = workspaces::add(c, &repo.display().to_string(), "repo")?;
                let m = missions::create(c, &missions::NewMission { workspace_id: Some(ws.id), title: "App".into(), briefing: "Build it".into(), mode: mode.into() })?;
                Ok(m.id)
            })
            .unwrap();
        (repo, db_path, mid)
    }

    fn set_maestro(db_path: &Path, mid: &str, mode: &str, max_parallel: i64, max_attempts: i64, max_runs: i64) {
        let db = crate::db::Db::open(db_path).unwrap();
        db.with(|c| {
            missions::set_maestro(c, mid, &missions::MaestroSettings { mode: mode.into(), max_parallel, require_review: true, max_attempts, max_runs, budget_usd: None })
        })
        .unwrap();
    }

    fn input(k: &str, title: &str, deps: Vec<String>) -> tasks::TaskInput {
        tasks::TaskInput { key: k.into(), title: title.into(), description: "x".into(), acceptance: "has fixed.txt".into(), profile_id: Some("builtin:backend".into()), depends_on: deps }
    }

    #[test]
    fn harness_picks_model_per_task_only_when_enabled() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "free");
        let engine = MissionEngine::with_options(db_path.clone(), tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)));
        engine.save_tasks(&mid, &[input("api", "Create API", vec![])]).unwrap();
        crate::db::Db::open(&db_path).unwrap().with(|c| missions::set_harness(c, &mid, true)).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "api review", |d| d.tasks[0].status == "review");
        let wt = PathBuf::from(d.tasks[0].worktree_path.clone().unwrap());
        assert_eq!(std::fs::read_to_string(wt.join("models.txt")).unwrap().trim(), "haiku", "small task → haiku");
        assert!(d.events.iter().any(|e| e.message.contains("Harness: modelo haiku")));

        // Without Harness the profile's (empty) model is kept: no --model flag.
        let tmp2 = tempfile::tempdir().unwrap();
        let (_r2, db2, mid2) = setup_repo_and_mission(tmp2.path(), "free");
        let e2 = MissionEngine::with_options(db2, tmp2.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp2.path())), Some(Arc::new(|_| None)));
        e2.save_tasks(&mid2, &[input("api", "Create API", vec![])]).unwrap();
        e2.start(&mid2).unwrap();
        let d2 = wait(&e2, &mid2, "api review", |d| d.tasks[0].status == "review");
        let wt2 = PathBuf::from(d2.tasks[0].worktree_path.clone().unwrap());
        assert!(!wt2.join("models.txt").exists());
    }

    #[test]
    fn agentic_maestro_reviews_retries_and_approves() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "agentic");
        let engine = MissionEngine::with_options(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)));
        engine.save_tasks(&mid, &[input("api", "Create API", vec![]), input("ui", "Create UI", vec!["api".into()])]).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "agentic completion", |d| d.mission.status == "completed");
        let api = &d.tasks[0];
        assert_eq!(api.attempts, 2, "rejected once by the reviewer, fixed on retry");
        assert_eq!(api.review_verdict.as_deref(), Some("approve"));
        assert_eq!(d.tasks[1].status, "approved");
        let msgs: Vec<&str> = d.events.iter().map(|e| e.message.as_str()).collect();
        assert!(msgs.iter().any(|m| m.contains("recomenda rejeitar")));
        assert!(msgs.iter().any(|m| m.contains("refazendo")));
        assert!(msgs.iter().any(|m| m.contains("aprovada automaticamente")));
        let report = d.mission.report.clone().expect("report generated on completion");
        assert!(report.contains("Create API") && report.contains("Revisão do Maestro (aprovou)"));
        assert!(report.contains("forge: Create API"), "report lists task commits");
        assert!(d.runs.len() >= 5, "2 api runs + 2 reviews + ui + review");
        // Integration is still a human gate.
        assert!(!repo.join("api.txt").exists());
        engine.integrate(&mid).unwrap();
        assert!(repo.join("ui.txt").exists());
    }

    #[test]
    fn agentic_escalates_after_retries_and_reassignment() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "agentic");
        set_maestro(&db_path, &mid, "agentic", 1, 2, 40);
        let probed = Arc::new(Mutex::new(Vec::<String>::new()));
        let p2 = probed.clone();
        let engine = MissionEngine::with_options(
            db_path,
            tmp.path().join("wt"),
            RunManager::default(),
            Arc::new(|_| {}),
            Some(fake_cli(tmp.path())),
            Some(Arc::new(move |cur: &str| {
                p2.lock().unwrap().push(cur.to_string());
                Some("codex".into())
            })),
        );
        engine.save_tasks(&mid, &[input("bad", "Unknown work", vec![])]).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "escalation", |d| d.events.iter().any(|e| e.message.contains("precisa da sua intervenção")));
        let t = &d.tasks[0];
        assert_eq!(t.status, "failed");
        assert_eq!(t.attempts, 3, "2 attempts + 1 after reassignment");
        assert_eq!(t.provider_override.as_deref(), Some("codex"));
        assert_eq!(probed.lock().unwrap().as_slice(), ["claude"], "probe asked once, excluding the current provider");
        assert!(d.events.iter().any(|e| e.message.contains("reatribuída para OpenAI Codex CLI")));
        assert_eq!(d.runs.iter().filter(|r| r.provider_id == "codex").count(), 1);
    }

    #[test]
    fn run_limit_pauses_the_mission() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "free");
        set_maestro(&db_path, &mid, "free", 2, 3, 1);
        let engine = MissionEngine::with_options(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)));
        engine.save_tasks(&mid, &[input("api", "Create API", vec![]), input("api2", "Create API", vec![])]).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "pause", |d| d.mission.status == "paused");
        assert!(d.events.iter().any(|e| e.kind == "blocked" && e.message.contains("limite de 1 execuções")));
        assert_eq!(d.tasks.iter().filter(|t| t.status != "pending").count(), 1, "only one task started");
        // Raising the limit and resuming continues.
        let c = crate::db::Db::open(&tmp.path().join("t.db")).unwrap();
        c.with(|c| missions::set_maestro(c, &mid, &missions::MaestroSettings { mode: "free".into(), max_parallel: 2, require_review: true, max_attempts: 3, max_runs: 10, budget_usd: None })).unwrap();
        engine.resume(&mid).unwrap();
        wait(&engine, &mid, "both reviewed", |d| d.tasks.iter().all(|t| t.status == "review"));
    }

    #[test]
    fn squad_restricts_planner_and_assignees() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "free");
        let db = crate::db::Db::open(&db_path).unwrap();
        db.with(|c| {
            crate::library::squads::ensure_builtins(c)?;
            missions::set_squad(c, &mid, Some("builtin:autopilot"))
        })
        .unwrap();
        drop(db);
        let engine = MissionEngine::with_options(db_path.clone(), tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)));
        let run = engine.plan(&mid).unwrap();
        // The fake plan uses roles "backend"/"frontend", which are not AUTOPILOT members.
        let d = wait(&engine, &mid, "planned", |d| d.mission.status == "planned");
        let members = ["builtin:planner", "builtin:researcher", "builtin:executor", "builtin:reviewer"];
        assert!(d.tasks.iter().all(|t| members.contains(&t.profile_id.as_deref().unwrap_or_default())), "{:?}", d.tasks.iter().map(|t| &t.profile_id).collect::<Vec<_>>());
        assert_eq!(run.profile_id.as_deref(), Some("builtin:planner"), "the squad's planner plans");
        assert!(run.prompt.contains("AUTOPILOT") && run.prompt.contains("role `executor`"));
    }

    #[test]
    fn mission_skills_reach_the_agent_and_are_recorded() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "free");
        let db = crate::db::Db::open(&db_path).unwrap();
        db.with(|c| {
            crate::skills::repo::ensure_builtins(c)?;
            crate::skills::repo::set_link(c, "builtin:testing-expert", "mission", &mid, true)?;
            crate::skills::repo::set_link(c, "builtin:react-expert", "mission", &mid, true)?;
            crate::skills::repo::set_enabled(c, "builtin:react-expert", false)?;
            Ok(())
        })
        .unwrap();
        drop(db);
        let engine = MissionEngine::with_options(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)));
        engine.save_tasks(&mid, &[input("api", "Create API", vec![])]).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "review", |d| d.tasks[0].status == "review");
        let wt = PathBuf::from(d.tasks[0].worktree_path.clone().unwrap());
        assert!(wt.join("skill-seen.txt").is_file(), "skill text was passed to the agent");
        assert!(!wt.join("disabled-skill-leaked.txt").exists(), "disabled skills are not injected");
        let run = d.runs.iter().find(|r| Some(&r.id) == d.tasks[0].run_id.as_ref()).unwrap();
        assert_eq!(run.skills, vec!["testing-expert@v1".to_string()]);
    }

    #[test]
    fn mcp_servers_are_attached_via_private_temp_file() {
        if crate::environment::which("git").is_none() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let (_repo, db_path, mid) = setup_repo_and_mission(tmp.path(), "free");
        let store: crate::mcp::secrets::Secrets = Arc::new(crate::mcp::secrets::MemoryStore::default());
        let db = crate::db::Db::open(&db_path).unwrap();
        db.with(|c| {
            let s = crate::mcp::repo::create(
                c,
                &crate::mcp::repo::ServerInput {
                    label: "Internal API".into(),
                    transport: "http".into(),
                    command: None,
                    args: vec![],
                    env: Default::default(),
                    url: Some("https://mcp.example.dev/mcp".into()),
                    headers: [("Authorization".to_string(), "Bearer ${API_KEY}".to_string())].into(),
                    secret_keys: vec!["API_KEY".into()],
                    read_only: true,
                    enabled: true,
                },
                None,
            )?;
            crate::mcp::repo::set_secret(c, store.as_ref(), &s.id, "API_KEY", "sk-test-very-secret-777")?;
            crate::mcp::repo::set_link(c, &s.id, "mission", &mid, true)
        })
        .unwrap();
        drop(db);
        let engine = MissionEngine::with_all(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}), Some(fake_cli(tmp.path())), Some(Arc::new(|_| None)), Some(store));
        engine.save_tasks(&mid, &[input("api", "Create API", vec![])]).unwrap();
        engine.start(&mid).unwrap();
        let d = wait(&engine, &mid, "review", |d| d.tasks[0].status == "review");
        let wt = PathBuf::from(d.tasks[0].worktree_path.clone().unwrap());
        let seen: Value = serde_json::from_str(&std::fs::read_to_string(wt.join("mcp-seen.json")).expect("agent received --mcp-config")).unwrap();
        assert_eq!(seen["mcpServers"]["internal-api"]["headers"]["Authorization"], "Bearer sk-test-very-secret-777");
        assert_eq!(std::fs::read_to_string(wt.join("mcp-perm.txt")).unwrap().trim(), "-rw-------", "owner-only file");
        let cfg_path = PathBuf::from(std::fs::read_to_string(wt.join("mcp-path.txt")).unwrap().trim());
        assert!(!cfg_path.exists(), "config with secrets deleted after the run");
        let run = d.runs.iter().find(|r| Some(&r.id) == d.tasks[0].run_id.as_ref()).unwrap();
        assert_eq!(run.mcp, vec!["internal-api".to_string()]);
        assert!(!run.prompt.contains("sk-test-very-secret-777"));
    }

    /// Real planning with Claude Code (haiku). `cargo test missions::engine::tests::live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_real_planning() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("README.md"), "# Tiny CLI\nA Python command-line tool (not started yet).\n").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "init"]);
        let db_path = tmp.path().join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        let mid = db
            .with(|c| {
                profiles::ensure_builtins(c, "claude")?;
                c.execute("UPDATE agent_profiles SET model = 'haiku' WHERE id = 'builtin:architect'", [])?;
                let ws = workspaces::add(c, &std::fs::canonicalize(&repo).unwrap().display().to_string(), "repo")?;
                let m = missions::create(
                    c,
                    &missions::NewMission {
                        workspace_id: Some(ws.id),
                        title: "Hello CLI".into(),
                        briefing: "Create hello.py that prints a greeting with an optional --name flag, plus a pytest test for it.".into(),
                        mode: "squad".into(),
                    },
                )?;
                Ok(m.id)
            })
            .unwrap();
        drop(db);
        let engine = MissionEngine::new(db_path, tmp.path().join("wt"), RunManager::default(), Arc::new(|_| {}));
        engine.plan(&mid).unwrap();
        let start = Instant::now();
        let d = loop {
            let d = engine.detail(&mid).unwrap();
            if !d.mission.planning {
                break d;
            }
            assert!(start.elapsed() < Duration::from_secs(300), "planning timeout");
            std::thread::sleep(Duration::from_millis(500));
        };
        for e in d.events.iter().rev() {
            eprintln!("event: {} — {}", e.kind, e.message);
        }
        for t in &d.tasks {
            eprintln!("task {} [{}] deps={} — {}", t.key, t.profile_id.clone().unwrap_or_default(), t.depends_on.len(), t.title);
        }
        eprintln!("totals: {:?}", d.totals);
        assert_eq!(d.mission.status, "planned");
        assert!(!d.tasks.is_empty());
    }

    /// Real Maestro review with Claude Code (haiku) of a commit with a deliberate bug.
    /// `cargo test missions::engine::tests::live_real_review -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_real_review() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("README.md"), "# calc").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "init"]);
        let base = git::head_commit(&repo).unwrap();
        std::fs::write(repo.join("calc.py"), "def add(a, b):\n    return a - b\n").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-q", "-m", "feat: add()"]);
        let task = Task {
            id: "t".into(), mission_id: "m".into(), position: 0, key: "add".into(), title: "Implement add()".into(),
            description: "Create calc.py with add(a, b) returning the sum.".into(), acceptance: "add(2, 3) == 5".into(),
            profile_id: None, depends_on: vec![], status: "review".into(), run_id: None, worktree_path: None, worktree_branch: None,
            attempts: 1, feedback: None, integrated_at: None, created_at: 0, updated_at: 0, base_commit: Some(base.clone()),
            review_run_id: None, review_verdict: None, review_summary: None, provider_override: None,
        };
        let db_path = tmp.path().join("t.db");
        let db = crate::db::Db::open(&db_path).unwrap();
        db.with(|c| { c.execute("INSERT INTO workspaces (id, name, path, created_at) VALUES ('w', 'w', '/tmp/w-live', 1)", [])?; Ok(()) }).unwrap();
        drop(db);
        let mgr = RunManager::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let req = RunRequest {
            workspace_id: "w".into(), profile_id: None, parent_run_id: None, provider_id: "claude".into(), model: Some("haiku".into()),
            permission: "read".into(), instructions: None, max_turns: None,
            prompt: maestro::review_prompt("Calc", &task, Some(&base)), resume_session: None, use_worktree: false, confirm_full: false,
            cwd_override: Some(std::fs::canonicalize(&repo).unwrap()), cwd_branch: None, skill_labels: vec![], mcp: None, mission_id: None, extra_skill_ids: vec![], extra_mcp_ids: vec![],
        };
        mgr.start_with_program(&db_path, &repo, tmp.path(), req, None, Some(Box::new(move |r| { let _ = tx.send(r); }))).unwrap();
        let run = rx.recv_timeout(Duration::from_secs(300)).expect("review finished");
        eprintln!("status={} cost={:?} text={:?}", run.status, run.cost_usd, run.result_text.as_deref().map(|t| &t[t.len().saturating_sub(400)..]));
        let review = maestro::parse_review(run.result_text.as_deref().unwrap_or_default()).expect("verdict in the agreed format");
        eprintln!("verdict approve={} summary={} issues={:?}", review.approve, review.summary, review.issues);
        assert!(!review.approve, "a - b must be rejected");
    }
}
