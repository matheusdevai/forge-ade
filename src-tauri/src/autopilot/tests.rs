use super::*;
use chrono::FixedOffset;
use std::sync::Mutex as StdMutex;

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<FixedOffset> {
    FixedOffset::east_opt(-3 * 3600).unwrap().with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
}

#[test]
fn schedules_interval_and_daily_with_weekdays() {
    let now = at(2026, 11, 3, 10, 0); // Tuesday
    assert_eq!(next_run("interval", Some(15), None, 127, &now), Some(now.timestamp_millis() + 15 * 60_000));
    // Daily 09:00 already passed today → tomorrow.
    assert_eq!(next_run("daily", None, Some("09:00"), 127, &now), Some(at(2026, 11, 4, 9, 0).timestamp_millis()));
    // Later today.
    assert_eq!(next_run("daily", None, Some("18:30"), 127, &now), Some(at(2026, 11, 3, 18, 30).timestamp_millis()));
    // Only Mondays (bit 0) → next Monday.
    assert_eq!(next_run("daily", None, Some("09:00"), 1, &now), Some(at(2026, 11, 9, 9, 0).timestamp_millis()));
    // Weekdays only, from Friday evening → Monday.
    assert_eq!(next_run("daily", None, Some("08:00"), 0b0011111, &at(2026, 11, 6, 20, 0)), Some(at(2026, 11, 9, 8, 0).timestamp_millis()));
    assert_eq!(next_run("manual", None, None, 127, &now), None);
    assert_eq!(next_run("commit", None, None, 127, &now), None);
    assert_eq!(next_run("daily", None, Some("25:00"), 127, &now), None);
}

#[test]
fn process_capture_exit_codes_timeout_and_redaction() {
    let no = AtomicBool::new(false);
    let mut ok = Command::new("/bin/sh");
    ok.args(["-c", "echo hello; echo token=ghp_abcdefghijklmnopqrstuvwxyz0123456789 1>&2"]);
    let (success, out) = run_process(ok, Duration::from_secs(10), &no).unwrap();
    assert!(success);
    assert!(out.contains("hello"));
    assert!(!out.contains("ghp_abcdefghijklmnop"), "secrets are redacted: {out}");
    let mut bad = Command::new("/bin/sh");
    bad.args(["-c", "echo boom; exit 3"]);
    let (success, out) = run_process(bad, Duration::from_secs(10), &no).unwrap();
    assert!(!success && out.contains("código de saída 3"));
    let mut slow = Command::new("/bin/sh");
    slow.args(["-c", "sleep 30 & sleep 30; echo never"]);
    let t = Instant::now();
    let e = run_process(slow, Duration::from_secs(1), &no).unwrap_err();
    assert!(t.elapsed() < Duration::from_secs(5), "process group killed on timeout");
    assert!(e.to_string().contains("tempo esgotado"));
}

struct Env {
    _tmp: tempfile::TempDir,
    db_path: PathBuf,
    ws_dir: PathBuf,
    ws_id: String,
    events: Arc<StdMutex<Vec<AutopilotEvent>>>,
    ap: Autopilot,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let ws_dir = std::fs::canonicalize(tmp.path()).unwrap().join("proj");
    std::fs::create_dir(&ws_dir).unwrap();
    std::fs::write(ws_dir.join("package.json"), r#"{"scripts":{"hello":"node -e \"console.log('hi from script')\""}}"#).unwrap();
    let db_path = tmp.path().join("t.db");
    let db = crate::db::Db::open(&db_path).unwrap();
    let ws_id = db
        .with(|c| {
            crate::agents::profiles::ensure_builtins(c, "claude")?;
            crate::library::squads::ensure_builtins(c)?;
            crate::library::recipes::ensure_builtins(c)?;
            Ok(workspaces::add(c, &ws_dir.display().to_string(), "proj")?.id)
        })
        .unwrap();
    drop(db);
    let events: Arc<StdMutex<Vec<AutopilotEvent>>> = Arc::default();
    let ev2 = events.clone();
    let ap = Autopilot::new(db_path.clone(), tmp.path().join("wt"), crate::agents::runner::RunManager::default(), None, None, Arc::new(move |e| ev2.lock().unwrap().push(e)));
    Env { _tmp: tmp, db_path, ws_dir, ws_id, events, ap }
}

fn input(ws: &str, action: ActionConfig) -> RoutineInput {
    RoutineInput {
        name: "R".into(),
        workspace_id: ws.into(),
        action,
        trigger: "manual".into(),
        interval_min: None,
        daily_time: None,
        days_mask: 127,
        max_runs_per_day: 24,
        timeout_min: 1,
        max_failures: 2,
        budget_usd_day: None,
        approve_command: true,
    }
}

fn cmd(c: &str) -> ActionConfig {
    ActionConfig::Command { command: c.into() }
}

fn wait_run(e: &Env, run_id: &str) -> RoutineRun {
    let start = Instant::now();
    loop {
        let r = get_run(&crate::db::connect(&e.db_path).unwrap(), run_id).unwrap();
        if r.status != "running" {
            return r;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "run did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn validation_least_privilege_and_command_approval() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let full = ActionConfig::Agent { profile_id: "builtin:frontend".into(), prompt: "x".into(), permission: "full".into(), use_worktree: false };
    assert!(matches!(create(&c, &input(&e.ws_id, full)), Err(AppError::Forbidden(_))));
    let mut i = input(&e.ws_id, cmd("echo ok"));
    i.approve_command = false;
    assert!(matches!(create(&c, &i), Err(AppError::Forbidden(_))), "unapproved command refused");
    i.approve_command = true;
    let r = create(&c, &i).unwrap();
    assert!(r.command_approved);
    // Editing the command without re-approval is refused; same command keeps approval.
    let mut edit = input(&e.ws_id, cmd("rm -rf ~/important"));
    edit.approve_command = false;
    assert!(update(&c, &r.id, &edit).is_err());
    let mut same = input(&e.ws_id, cmd("echo ok"));
    same.approve_command = false;
    same.name = "Renamed".into();
    assert!(update(&c, &r.id, &same).unwrap().command_approved);
    // A tampered command in the DB is detected at run time.
    c.execute("UPDATE routines SET config = ?2 WHERE id = ?1", params![r.id, serde_json::to_string(&cmd("echo pwned")).unwrap()]).unwrap();
    let tampered = get(&c, &r.id).unwrap();
    assert!(!tampered.command_approved);
    assert!(gate(&c, &tampered).unwrap().unwrap().contains("aprovado"));
    // Schedule bounds and script existence.
    let mut fast = input(&e.ws_id, cmd("echo ok"));
    fast.trigger = "interval".into();
    fast.interval_min = Some(1);
    assert!(create(&c, &fast).is_err());
    assert!(create(&c, &input(&e.ws_id, ActionConfig::Script { script: "missing".into() })).is_err());
    assert!(create(&c, &input(&e.ws_id, ActionConfig::Verify { url: "file:///etc/passwd".into() })).is_err());
}

#[test]
fn command_and_script_run_record_and_notify() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let r = create(&c, &input(&e.ws_id, cmd("pwd; echo FORGE-OK"))).unwrap();
    let run = e.ap.trigger(&r.id, "manual").unwrap();
    let done = wait_run(&e, &run.id);
    assert_eq!(done.status, "succeeded", "{:?}", done.error);
    let out = done.output.unwrap();
    assert!(out.contains("FORGE-OK") && out.contains(&e.ws_dir.display().to_string()), "runs in the workspace: {out}");
    if crate::environment::which("npm").is_some() && crate::environment::which("node").is_some() {
        let s = create(&c, &input(&e.ws_id, ActionConfig::Script { script: "hello".into() })).unwrap();
        let d = wait_run(&e, &e.ap.trigger(&s.id, "manual").unwrap().id);
        assert_eq!(d.status, "succeeded", "{:?} {:?}", d.error, d.output);
        assert!(d.output.unwrap().contains("hi from script"));
    }
    let evs = e.events.lock().unwrap();
    assert!(evs.iter().any(|x| x.status == "running") && evs.iter().any(|x| x.status == "succeeded"));
}

#[test]
fn no_overlap_daily_cap_and_auto_disable() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let slow = create(&c, &input(&e.ws_id, cmd("sleep 2"))).unwrap();
    let first = e.ap.trigger(&slow.id, "manual").unwrap();
    let second = e.ap.trigger(&slow.id, "manual").unwrap();
    assert_eq!(second.status, "skipped");
    assert!(second.error.unwrap().contains("em andamento"));
    assert_eq!(wait_run(&e, &first.id).status, "succeeded");

    let mut capped = input(&e.ws_id, cmd("true"));
    capped.max_runs_per_day = 1;
    let capped = create(&c, &capped).unwrap();
    wait_run(&e, &e.ap.trigger(&capped.id, "manual").unwrap().id);
    let over = e.ap.trigger(&capped.id, "manual").unwrap();
    assert_eq!(over.status, "skipped");
    assert!(over.error.unwrap().contains("limite diário"));

    let failing = create(&c, &input(&e.ws_id, cmd("echo nope; exit 1"))).unwrap();
    for _ in 0..2 {
        let d = wait_run(&e, &e.ap.trigger(&failing.id, "manual").unwrap().id);
        assert_eq!(d.status, "failed");
        assert!(d.output.unwrap().contains("nope"));
    }
    let after = get(&c, &failing.id).unwrap();
    assert!(!after.enabled, "auto-disabled after max_failures");
    assert!(after.disabled_reason.unwrap().contains("2 falhas"));
    assert!(e.events.lock().unwrap().iter().any(|x| x.auto_disabled));
    // Re-enabling resets the counter.
    assert_eq!(set_enabled(&c, &failing.id, true).unwrap().consecutive_failures, 0);
}

#[test]
fn budget_counts_only_reported_costs() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let mut i = input(&e.ws_id, cmd("true"));
    i.budget_usd_day = Some(0.05);
    let r = create(&c, &i).unwrap();
    c.execute("INSERT INTO routine_runs (id, routine_id, trigger, status, cost_usd, started_at, ended_at) VALUES ('x', ?1, 'manual', 'succeeded', NULL, ?2, ?2)", params![r.id, now_ms()]).unwrap();
    assert_eq!(gate(&c, &get(&c, &r.id).unwrap()).unwrap(), None, "unknown cost is not invented");
    c.execute("INSERT INTO routine_runs (id, routine_id, trigger, status, cost_usd, started_at, ended_at) VALUES ('y', ?1, 'manual', 'succeeded', 0.06, ?2, ?2)", params![r.id, now_ms()]).unwrap();
    assert!(gate(&c, &get(&c, &r.id).unwrap()).unwrap().unwrap().contains("orçamento"));
}

#[test]
fn scheduler_fires_due_routines_once_and_reschedules() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let mut i = input(&e.ws_id, cmd("echo tick"));
    i.trigger = "interval".into();
    i.interval_min = Some(5);
    let r = create(&c, &i).unwrap();
    assert!(r.next_run_at.unwrap() > now_ms());
    assert!(e.ap.tick().unwrap().is_empty(), "not due yet");
    c.execute("UPDATE routines SET next_run_at = ?2 WHERE id = ?1", params![r.id, now_ms() - 1000]).unwrap();
    let fired = e.ap.tick().unwrap();
    assert_eq!(fired.len(), 1);
    assert!(get(&c, &r.id).unwrap().next_run_at.unwrap() > now_ms() + 4 * 60_000, "rescheduled before running");
    assert!(e.ap.tick().unwrap().is_empty(), "no burst");
    assert_eq!(wait_run(&e, &fired[0].id).status, "succeeded");
    // Disabled routines never fire.
    set_enabled(&c, &r.id, false).unwrap();
    c.execute("UPDATE routines SET next_run_at = ?2 WHERE id = ?1", params![r.id, now_ms() - 1000]).unwrap();
    assert!(e.ap.tick().unwrap().is_empty());
}

#[test]
fn commit_trigger_fires_on_new_commits_only() {
    if crate::environment::which("git").is_none() {
        return;
    }
    let e = env();
    let git = |args: &[&str]| {
        let out = crate::git::run(&e.ws_dir, args, None, Duration::from_secs(30)).unwrap();
        assert!(out.success, "{:?}: {}", args, out.stderr);
    };
    git(&["init", "-q", "-b", "main"]);
    for (k, v) in [("user.name", "T"), ("user.email", "t@t.dev"), ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")] {
        git(&["config", k, v]);
    }
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "one"]);
    let c = crate::db::connect(&e.db_path).unwrap();
    let mut i = input(&e.ws_id, cmd("git log -1 --format=%s"));
    i.trigger = "commit".into();
    let r = create(&c, &i).unwrap();
    assert!(e.ap.tick().unwrap().is_empty(), "first observation = baseline");
    assert!(e.ap.tick().unwrap().is_empty());
    std::fs::write(e.ws_dir.join("a.txt"), "x").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "second commit"]);
    let fired = e.ap.tick().unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].trigger, "commit");
    let d = wait_run(&e, &fired[0].id);
    assert!(d.output.unwrap().contains("second commit"));
    assert!(e.ap.tick().unwrap().is_empty());
    let _ = r;
}

#[cfg(unix)]
#[test]
fn agent_routine_runs_through_the_agent_engine() {
    use std::os::unix::fs::PermissionsExt;
    let e = env();
    let fake = e.ws_dir.parent().unwrap().join("fake-claude.sh");
    std::fs::write(
        &fake,
        "#!/bin/sh\nprev=\"\"; for a in \"$@\"; do if [ \"$prev\" = \"--permission-mode\" ]; then echo \"$a\" > ../perm.txt; fi; prev=\"$a\"; done\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"routine agent done\",\"duration_ms\":5,\"num_turns\":1,\"total_cost_usd\":0.002}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let ap = Autopilot::new(e.db_path.clone(), e.ws_dir.parent().unwrap().join("wt"), crate::agents::runner::RunManager::default(), None, None, Arc::new(|_| {})).with_agent_program(fake);
    let c = crate::db::connect(&e.db_path).unwrap();
    let r = create(&c, &input(&e.ws_id, ActionConfig::Agent { profile_id: "builtin:reviewer".into(), prompt: "Review".into(), permission: "read".into(), use_worktree: false })).unwrap();
    let d = wait_run(&e, &ap.trigger(&r.id, "manual").unwrap().id);
    assert_eq!(d.status, "succeeded", "{:?}", d.error);
    assert_eq!(d.output.as_deref(), Some("routine agent done"));
    assert_eq!(d.cost_usd, Some(0.002), "reported cost recorded");
    let agent_run = crate::agents::runs::get(&c, d.agent_run_id.as_deref().unwrap()).unwrap();
    assert_eq!(agent_run.permission, "read");
}

#[test]
fn orphan_runs_are_closed_on_startup() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    let r = create(&c, &input(&e.ws_id, cmd("true"))).unwrap();
    c.execute("INSERT INTO routine_runs (id, routine_id, trigger, status, started_at) VALUES ('o', ?1, 'schedule', 'running', 1)", [&r.id]).unwrap();
    assert_eq!(mark_orphans(&c).unwrap(), 1);
    assert_eq!(get_run(&c, "o").unwrap().status, "cancelled");
}

/// Real Claude Code through a routine (read-only, haiku).
#[test]
#[ignore = "uses the Claude Code CLI"]
fn live_agent_routine_with_claude() {
    let e = env();
    let c = crate::db::connect(&e.db_path).unwrap();
    c.execute("UPDATE agent_profiles SET model = 'haiku', max_turns = 2 WHERE id = 'builtin:reviewer'", []).unwrap();
    let r = create(&c, &input(&e.ws_id, ActionConfig::Agent { profile_id: "builtin:reviewer".into(), prompt: "Reply with exactly: AUTOPILOT-OK".into(), permission: "read".into(), use_worktree: false })).unwrap();
    let mut done = None;
    let run = e.ap.trigger(&r.id, "manual").unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        let cur = get_run(&c, &run.id).unwrap();
        if cur.status != "running" {
            done = Some(cur);
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let d = done.expect("finished");
    eprintln!("{:?}", d);
    assert_eq!(d.status, "succeeded", "{:?}", d.error);
    assert!(d.output.unwrap().contains("AUTOPILOT-OK"));
}
