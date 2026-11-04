//! Observability commands (M19).

use chrono::{Local, TimeZone};
use tauri::State;

use super::{blocking, AppPaths};
use crate::agents::runner::RunManager;
use crate::db::{now_ms, Db};
use crate::error::{AppError, AppResult};
use crate::observability::{self, Metrics, ProcessInfo, ReplayStep, TimelineItem, TimelineQuery, DAY_MS};
use crate::terminal::TerminalManager;

fn local_midnight(ms: i64) -> i64 {
    let d = Local.timestamp_millis_opt(ms).single().unwrap_or_else(Local::now).date_naive();
    Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().map(|x| x.timestamp_millis()).unwrap_or(ms)
}

/// Window → (since, local day starts for the daily chart).
pub fn window(range: &str, now: i64) -> AppResult<(i64, Vec<i64>)> {
    let days = match range {
        "24h" => return Ok((now - DAY_MS, vec![local_midnight(now - DAY_MS), local_midnight(now)])),
        "7d" => 7,
        "30d" => 30,
        "90d" => 90,
        r => return Err(AppError::Invalid(format!("período inválido: {r}"))),
    };
    let first = local_midnight(now - (days - 1) * DAY_MS);
    let mut starts = Vec::new();
    let mut t = first;
    while t <= now {
        starts.push(t);
        // +25h then snap to midnight: safe across DST changes.
        t = local_midnight(t + DAY_MS + 3_600_000);
    }
    starts.dedup();
    Ok((first, starts))
}

#[tauri::command]
pub async fn obs_metrics(paths: State<'_, AppPaths>, range: String, workspace_id: Option<String>) -> AppResult<Metrics> {
    let p = paths.db_path.clone();
    blocking(move || {
        let (since, days) = window(&range, now_ms())?;
        let c = crate::db::connect(&p)?;
        observability::metrics(&c, since, workspace_id.as_deref(), &days)
    })
    .await
}

#[tauri::command]
pub async fn obs_timeline(paths: State<'_, AppPaths>, query: TimelineQuery) -> AppResult<Vec<TimelineItem>> {
    let p = paths.db_path.clone();
    blocking(move || observability::timeline(&crate::db::connect(&p)?, &query)).await
}

#[tauri::command]
pub fn obs_replay(db: State<'_, Db>, run_id: String) -> AppResult<Vec<ReplayStep>> {
    db.with(|c| observability::replay(c, &run_id))
}

#[tauri::command]
pub async fn obs_processes(
    db: State<'_, Db>,
    terminals: State<'_, TerminalManager>,
    runs: State<'_, RunManager>,
    browser: State<'_, super::browser::Browser>,
    voice: State<'_, super::voice::Voice>,
) -> AppResult<Vec<ProcessInfo>> {
    let mut out: Vec<ProcessInfo> = Vec::new();
    out.push(ProcessInfo {
        kind: "app".into(),
        label: "FORGE".into(),
        workspace_id: None,
        pid: Some(std::process::id()),
        started_at: None,
        cpu_percent: None,
        rss_kb: None,
        ref_id: None,
    });
    for t in terminals.list(None).into_iter().filter(|t| !t.exited) {
        let shell = std::path::Path::new(&t.shell).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(t.shell.clone());
        out.push(ProcessInfo {
            kind: "terminal".into(),
            label: t.provider_id.clone().map(|p| format!("{p} (CLI interativo)")).unwrap_or(shell),
            workspace_id: Some(t.workspace_id.clone()),
            pid: t.pid,
            started_at: Some(t.created_at),
            cpu_percent: None,
            rss_kb: None,
            ref_id: Some(t.id.clone()),
        });
    }
    for (id, pid) in runs.live_pids() {
        let r = db.with(|c| crate::agents::runs::get(c, &id)).ok();
        out.push(ProcessInfo {
            kind: "agent".into(),
            label: r.as_ref().map(|r| format!("{} · {}", r.provider_id, r.prompt.chars().take(60).collect::<String>())).unwrap_or_else(|| "agente".into()),
            workspace_id: r.as_ref().and_then(|r| r.workspace_id.clone()),
            pid,
            started_at: r.as_ref().map(|r| r.started_at),
            cpu_percent: None,
            rss_kb: None,
            ref_id: Some(id),
        });
    }
    if let Some(pid) = browser.pid() {
        let st = browser.status();
        out.push(ProcessInfo {
            kind: "browser".into(),
            label: format!("{} headless · {} aba(s)", st.name.unwrap_or_else(|| "Chrome".into()), st.tabs),
            workspace_id: None,
            pid: Some(pid),
            started_at: None,
            cpu_percent: None,
            rss_kb: None,
            ref_id: None,
        });
    }
    let running_routines = db.with(|c| {
        let mut st = c.prepare("SELECT rr.id, r.name, r.workspace_id, rr.started_at FROM routine_runs rr JOIN routines r ON r.id = rr.routine_id WHERE rr.status = 'running'")?;
        let v = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    })?;
    for (id, name, ws, started) in running_routines {
        out.push(ProcessInfo { kind: "routine".into(), label: format!("Rotina “{name}”"), workspace_id: Some(ws), pid: None, started_at: Some(started), cpu_percent: None, rss_kb: None, ref_id: Some(id) });
    }
    let v = voice.inner().clone();
    let listening = blocking(move || Ok(v.status().listening)).await.unwrap_or(false);
    if listening {
        out.push(ProcessInfo { kind: "voice".into(), label: "Reconhecimento de voz (microfone aberto)".into(), workspace_id: None, pid: None, started_at: None, cpu_percent: None, rss_kb: None, ref_id: None });
    }
    let pids: Vec<u32> = out.iter().filter_map(|p| p.pid).collect();
    let stats = blocking(move || Ok(observability::ps_stats(&pids))).await?;
    for p in &mut out {
        if let Some((cpu, rss)) = p.pid.and_then(|pid| stats.get(&pid)) {
            p.cpu_percent = Some(*cpu);
            p.rss_kb = Some(*rss);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_cover_local_days() {
        let now = now_ms();
        let (since, days) = window("7d", now).unwrap();
        assert_eq!(days.len(), 7);
        assert_eq!(since, days[0]);
        assert!(days.windows(2).all(|w| w[1] > w[0]));
        assert!(*days.last().unwrap() <= now);
        let (s24, d24) = window("24h", now).unwrap();
        assert_eq!(s24, now - DAY_MS);
        assert!(d24.len() == 2);
        assert_eq!(window("30d", now).unwrap().1.len(), 30);
        assert!(window("1y", now).is_err());
    }
}
