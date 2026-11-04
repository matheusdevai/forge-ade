//! FORGE desktop core.

pub mod agents;
pub mod autopilot;
pub mod browser;
pub mod commands;
pub mod crash;
pub mod db;
pub mod environment;
pub mod error;
pub mod git;
pub mod harness;
pub mod library;
pub mod mcp;
pub mod media;
pub mod memory;
pub mod missions;
pub mod observability;
pub mod providers;
pub mod redact;
pub mod search;
pub mod skills;
pub mod terminal;
pub mod voice;
pub mod workspace_fs;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the running window instead of
        // starting another instance (shared DB, browser profile, scheduler).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            crash::install_panic_hook(data_dir.join("crash"));
            let db_path = data_dir.join("forge.db");
            let db = db::Db::open(&db_path)?;
            db.with(|c| {
                agents::profiles::ensure_builtins(c, "claude")?;
                library::squads::ensure_builtins(c)?;
                library::recipes::ensure_builtins(c)?;
                skills::repo::ensure_builtins(c)?;
                agents::runs::mark_orphans(c)?;
                Ok(())
            })?;
            app.manage(db);
            let run_manager = agents::runner::RunManager::default();
            let handle = app.handle().clone();
            let engine = missions::engine::MissionEngine::new(
                db_path.clone(),
                data_dir.join("worktrees"),
                run_manager.clone(),
                std::sync::Arc::new(move |mission_id: &str| {
                    let _ = tauri::Emitter::emit(&handle, "forge://mission-changed", mission_id.to_string());
                }),
            );
            app.manage(run_manager);
            app.manage(commands::PendingSkills::default());
            let secrets: mcp::secrets::Secrets = std::sync::Arc::new(mcp::secrets::KeychainStore);
            app.manage(secrets);
            app.manage(engine);
            let browser_mgr: commands::browser::Browser = std::sync::Arc::new(browser::manager::BrowserManager::new(data_dir.clone()));
            app.manage(browser_mgr.clone());
            // Autopilot (M18): background scheduler; events + OS notifications on failure.
            let ap_handle = app.handle().clone();
            let autopilot = autopilot::Autopilot::new(
                db_path.clone(),
                data_dir.join("worktrees"),
                app.state::<agents::runner::RunManager>().inner().clone(),
                Some(app.state::<missions::engine::MissionEngine>().inner().clone()),
                Some(browser_mgr),
                std::sync::Arc::new(move |ev: autopilot::AutopilotEvent| {
                    if ev.status == "failed" {
                        use tauri_plugin_notification::NotificationExt;
                        let body = if ev.auto_disabled {
                            format!("“{}” falhou e foi desativada após falhas seguidas.", ev.routine_name)
                        } else {
                            format!("“{}” falhou: {}", ev.routine_name, ev.message.clone().unwrap_or_default().chars().take(140).collect::<String>())
                        };
                        let _ = ap_handle.notification().builder().title("FORGE Autopilot").body(body).show();
                    }
                    let _ = tauri::Emitter::emit(&ap_handle, "forge://autopilot-changed", ev);
                }),
            );
            autopilot.start();
            app.manage(autopilot);
            app.manage(std::sync::Arc::new(voice::VoiceManager::default()) as commands::voice::Voice);
            app.manage(std::sync::Arc::new(media::MediaEngine::new(data_dir.clone())) as commands::media::Media);
            app.manage(commands::media::PickedFiles::default());
            app.manage(commands::AppPaths { data_dir, db_path });
            app.manage(terminal::TerminalManager::default());
            // Warm the login-shell PATH cache off the main thread.
            std::thread::spawn(|| {
                let _ = environment::search_path();
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::reveal_data_dir,
            commands::detect_environment,
            commands::settings_get_all,
            commands::settings_set,
            commands::workspaces_list,
            commands::workspaces_add,
            commands::workspaces_open,
            commands::workspaces_set_pinned,
            commands::workspaces_rename,
            commands::workspaces_remove,
            commands::workspaces_reveal,
            commands::fs_list_dir,
            commands::fs_read_text,
            commands::fs_write_text,
            commands::fs_stat,
            commands::fs_create,
            commands::fs_rename,
            commands::fs_trash,
            commands::fs_list_files,
            commands::fs_search,
            commands::workspace_git_info,
            commands::sessions_list,
            commands::sessions_create,
            commands::sessions_rename,
            commands::sessions_save_layout,
            commands::sessions_set_active,
            commands::sessions_delete,
            commands::sessions_reorder,
            commands::presets_list,
            commands::presets_create,
            commands::presets_rename,
            commands::presets_delete,
            commands::providers_list,
            commands::provider_status,
            commands::provider_verify,
            commands::terminal_spawn,
            commands::terminal_attach,
            commands::terminal_detach,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_kill,
            commands::terminal_list,
            commands::git_status,
            commands::git_init,
            commands::git_file_contents,
            commands::git_stage,
            commands::git_unstage,
            commands::git_discard,
            commands::git_stage_content,
            commands::git_commit,
            commands::git_log,
            commands::git_commit_files,
            commands::git_branches,
            commands::git_switch,
            commands::git_delete_branch,
            commands::git_merge,
            commands::git_merge_abort,
            commands::git_fetch,
            commands::git_pull,
            commands::git_push,
            commands::git_stash_list,
            commands::git_stash_push,
            commands::git_stash_action,
            commands::git_worktrees,
            commands::git_worktree_add,
            commands::git_worktree_remove,
            commands::agent_profiles_list,
            commands::agent_profile_create,
            commands::agent_profile_update,
            commands::agent_profile_delete,
            commands::agent_profile_restore,
            commands::agent_run_start,
            commands::agent_run_attach,
            commands::agent_run_detach,
            commands::agent_run_cancel,
            commands::agent_runs_list,
            commands::agent_run_delete,
            commands::agent_run_changes,
            commands::squads_list,
            commands::squad_create,
            commands::squad_update,
            commands::squad_delete,
            commands::squad_restore,
            commands::recipes_list,
            commands::recipe_create,
            commands::recipe_update,
            commands::recipe_delete,
            commands::recipe_restore,
            commands::mission_from_recipe,
            commands::recipe_from_mission,
            commands::mission_set_squad,
            commands::bundle_export,
            commands::bundle_open,
            commands::bundle_import,
            commands::skills_list,
            commands::skill_detail,
            commands::skill_create,
            commands::skill_update,
            commands::skill_restore_version,
            commands::skill_restore_builtin,
            commands::skill_set_enabled,
            commands::skill_delete,
            commands::skill_set_link,
            commands::skills_linked,
            commands::skill_pick,
            commands::skill_install,
            commands::skill_export,
            commands::mcp_list,
            commands::mcp_detail,
            commands::mcp_templates,
            commands::mcp_create,
            commands::mcp_create_from_template,
            commands::mcp_update,
            commands::mcp_delete,
            commands::mcp_set_enabled,
            commands::mcp_set_secret,
            commands::mcp_clear_secret,
            commands::mcp_test,
            commands::mcp_set_link,
            commands::mcp_linked,
            commands::mission_detail,
            commands::mission_update,
            commands::mission_maestro,
            commands::mission_plan,
            commands::mission_save_tasks,
            commands::mission_start,
            commands::mission_pause,
            commands::mission_resume,
            commands::mission_cancel_run,
            commands::mission_integrate,
            commands::mission_cleanup,
            commands::mission_task_action,
            commands::missions_list,
            commands::missions_create,
            commands::missions_cancel,
            commands::missions_delete,
            commands::activity_recent,
            commands::voice::voice_status,
            commands::voice::voice_authorize,
            commands::voice::voice_locales,
            commands::voice::voice_listen,
            commands::voice::voice_stop,
            commands::voice::voice_cancel,
            commands::voice::voice_speak,
            commands::voice::voice_stop_speaking,
            commands::voice::voice_self_test,
            commands::voice::agents_pause_all,
            commands::harness::harness_recommend,
            commands::harness::harness_usage,
            commands::harness::mission_set_harness,
            commands::app::app_check_update,
            commands::app::app_install_update,
            commands::app::crash_list,
            commands::app::crash_read,
            commands::app::crash_clear,
            commands::app::crash_report_ui,
            commands::app::crash_reveal,
            commands::observability::obs_metrics,
            commands::observability::obs_timeline,
            commands::observability::obs_replay,
            commands::observability::obs_processes,
            commands::autopilot::autopilot_list,
            commands::autopilot::autopilot_create,
            commands::autopilot::autopilot_update,
            commands::autopilot::autopilot_set_enabled,
            commands::autopilot::autopilot_delete,
            commands::autopilot::autopilot_run_now,
            commands::autopilot::autopilot_cancel,
            commands::autopilot::autopilot_runs,
            commands::media::media_status,
            commands::media::media_set_key,
            commands::media::media_clear_key,
            commands::media::media_test,
            commands::media::media_set_model,
            commands::media::media_pick,
            commands::media::media_generate_image,
            commands::media::media_analyze_image,
            commands::media::media_tts,
            commands::media::media_transcribe,
            commands::media::media_video_start,
            commands::media::media_list,
            commands::media::media_read,
            commands::media::media_delete,
            commands::media::media_reveal,
            commands::media::media_save_to_workspace,
            commands::memory::memory_list,
            commands::memory::memory_counts,
            commands::memory::memory_create,
            commands::memory::memory_update,
            commands::memory::memory_set_status,
            commands::memory::memory_set_pinned,
            commands::memory::memory_approve,
            commands::memory::memory_delete,
            commands::memory::memory_preview,
            commands::browser::browser_status,
            commands::browser::browser_open,
            commands::browser::browser_list,
            commands::browser::browser_attach,
            commands::browser::browser_detach,
            commands::browser::browser_navigate,
            commands::browser::browser_go,
            commands::browser::browser_reload,
            commands::browser::browser_nav_state,
            commands::browser::browser_viewport,
            commands::browser::browser_mouse,
            commands::browser::browser_key,
            commands::browser::browser_insert_text,
            commands::browser::browser_console,
            commands::browser::browser_console_clear,
            commands::browser::browser_screenshot,
            commands::browser::browser_verify,
            commands::browser::browser_close,
            commands::browser::browser_reveal_shot,
            commands::browser::preview_detect,
            commands::browser::preview_start,
        ])
        .build(tauri::generate_context!())
        .expect("error while building FORGE")
        .run(|app, event| {
            // Never leave orphan shells behind when the app quits.
            if let tauri::RunEvent::Exit = event {
                app.state::<terminal::TerminalManager>().kill_all();
                app.state::<agents::runner::RunManager>().cancel_all();
                app.state::<commands::browser::Browser>().kill_all();
                app.state::<commands::voice::Voice>().shutdown();
                app.state::<autopilot::Autopilot>().stop();
            }
        });
}
