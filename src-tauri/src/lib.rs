pub mod agents;
pub mod appdir;
pub mod applog;
pub mod apps;
pub mod atomic;
pub mod audit;
pub mod certs;
pub mod commands;
pub mod compose_policy;
pub mod config;
pub mod connect;
pub mod contracts;
pub mod crash;
pub mod daemon;
pub mod db;
pub mod debugbridge;
pub mod detect;
pub mod devserver;
pub mod diagnostics;
pub mod doctor;
pub mod elevate;
pub mod engine;
pub mod env_writer;
pub mod error;
pub mod events;
pub mod generator;
pub mod git;
pub mod handover;
pub mod hints;
pub mod hosts;
pub mod imports;
pub mod inflight;
pub mod instances;
pub mod licences;
pub mod locale;
pub mod logging;
pub mod mail;
pub mod manifest;
pub mod market;
pub mod mcp;
pub mod menu;
pub mod migrate;
pub mod paths;
pub mod phpini;
pub mod pkg;
pub mod policy;
pub mod ports;
pub mod preflight;
pub mod preset;
pub mod profile;
pub mod progress;
pub mod pty;
pub mod quickcmd;
pub mod release;
pub mod render;
pub mod runner;
pub mod scaffold;
pub mod secrets;
pub mod skeleton;
pub mod snapshot;
pub mod stats;
pub mod stats_store;
pub mod template;
pub mod tray;
pub mod tunnel;
pub mod watcher;
pub mod worker;
pub mod workspace;
pub mod xdebug;

use commands::AppState;

/// The label Tauri gives the window declared in `tauri.conf.json`.
///
/// Public because the window event handler compares against it and three other
/// modules look the window up by it; a second spelling somewhere would make
/// the guard below quietly stop guarding.
pub const MAIN_WINDOW: &str = "main";
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // First, before the log itself: `logging::init` can fail, and a panic
    // inside it would otherwise be the one crash with nowhere to be recorded.
    // The hook needs no subscriber — it writes its report with `fs::write`,
    // and `logging::dir()` is a path, not state that `init` sets up.
    crash::install();

    // Before anything else: a failure during plugin setup is exactly the kind
    // that used to leave no trace. Held for the process lifetime — dropping the
    // guard stops the writer and discards whatever is buffered.
    let _log_guard = logging::init();

    // After the log, so the move has somewhere to be recorded, and before the
    // first command can read a preference: the folders these live in were named
    // after the bundle identifier until this release, and an install that came
    // through the old name has settings worth keeping.
    appdir::migrate_config();

    tauri::Builder::default()
        // A second launch focuses the existing window instead of opening a
        // rival instance: two apps driving the same Docker stack and the same
        // .env would race each other.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::new())
        .manage(pty::Registry::new())
        .setup(|app| {
            let handle = app.handle().clone();

            tray::build(&handle)?;

            // Localised from the same preference the tray reads, so the menu
            // bar does not sit in English beside a Turkish window.
            //
            // These are the built-in two languages only, and deliberately: the
            // webview has not booted yet, so the front end's catalog cannot have
            // arrived. It replaces this the moment it does, through
            // `tray_relabel` — which is what keeps a third language out of this
            // file.
            let labels = tray::menu_labels();
            // From the config rather than spelled again here: the menu bar showing a
            // different name from the bundle is how `stackvo-desktop` ended up in it.
            let product = app
                .config()
                .product_name
                .clone()
                .unwrap_or_else(|| "StackVo".to_string());
            app.set_menu(menu::build(&handle, &labels, &product)?)?;

            // Fill the screen the window opened on.
            //
            // `"maximized": true` in the config did not do it, and paired with
            // `"center": true` it produced the shape in the bug report: a
            // window sized for the screen, then re-centred on the size it had
            // before, hanging off the right edge. Both of those are gone.
            //
            // The work area rather than the monitor: on macOS the monitor
            // includes the menu bar and the dock, and a window set to it sits
            // underneath both. `maximize()` is the fallback for the case where
            // no monitor answers — a window on no screen at all, which happens
            // while displays are being reconfigured.
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                match window.current_monitor() {
                    Ok(Some(monitor)) => {
                        let area = monitor.work_area();
                        let _ = window.set_position(area.position);
                        let _ = window.set_size(area.size);
                    }
                    _ => {
                        let _ = window.maximize();
                    }
                }
            }

            // `startMinimized` has been in the preference defaults all along
            // and was never read. With a tray that survives a window close, it
            // finally has somewhere to start minimised *to*.
            if commands::start_minimized() {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let _ = window.hide();
                }
            }

            // Watch the selected checkout. `workspace_set` retargets this
            // handle, so changing the workspace moves the watcher with it
            // instead of leaving it on the directory the user just left.
            let watcher = watcher::Handle::new();
            {
                let state = app.state::<AppState>();
                let root = commands::recover(&state.workspace).require_root().ok();
                watcher.retarget(&handle, root);
            }
            app.manage(watcher);

            // Scheduled database snapshots.
            //
            // A five-minute tick rather than a timer set to the interval: the
            // machine sleeps, and a `sleep(24h)` started before the lid closed
            // fires a day late. Each tick asks when the last automatic snapshot
            // was taken and compares it with the clock, so a laptop that was
            // shut for three days owes one snapshot rather than three.
            //
            // Does nothing at all unless a schedule has been chosen, which is
            // the default — see `snapshot_settings`.
            let backup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    // Before the first check, not after: the app has just
                    // started, the engine may not be up yet, and a dump racing
                    // the stack's own boot is a failed one.
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                    commands::run_due_snapshots(&backup_handle).await;
                }
            });

            // Slow tray refresh. Deliberately lazy: this is a glanceable
            // summary, not a dashboard, and hammering the daemon from a
            // background timer would be rude.
            let tray_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tray::refresh(tray_handle.clone()).await;
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                }
            });

            // Push, not poll. The engine broadcasts container transitions and
            // its own availability; listening beats refetching on a timer, and
            // it is what lets the UI react to a container dying on its own.
            let events_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut last_reachable: Option<bool> = None;
                loop {
                    let status = engine::status().await;
                    if last_reachable != Some(status.reachable) {
                        last_reachable = Some(status.reachable);
                        // "Docker was down at 14:02 and back at 14:05" answers
                        // most of the reports that arrive as "it stopped working".
                        tracing::info!(
                            reachable = status.reachable,
                            socket = ?status.socket_path,
                            error = ?status.error,
                            "engine reachability changed"
                        );
                        events::emit(&events_handle, "engine:status_changed", status.clone());
                    }

                    if status.reachable {
                        let emitter = events_handle.clone();
                        // Returns when the connection drops — normal on a
                        // Docker restart, so we simply reconnect below.
                        let _ = engine::watch_container_events(move |name, action, running| {
                            let id = name
                                .strip_prefix(engine::CONTAINER_PREFIX)
                                .unwrap_or(&name)
                                .to_string();
                            events::emit(
                                &emitter,
                                "container:state_changed",
                                serde_json::json!({
                                    "name": name, "id": id,
                                    "state": action, "running": running
                                }),
                            );
                        })
                        .await;
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            });

            // Per-container history, so a freshly opened detail view has a
            // sparkline instead of a single point. The web UI sampled this too,
            // but kept it in the dashboard container, so it died on restart.
            //
            // The interval follows the window: nobody is reading a sparkline
            // that is not on screen, and this was the app's only unattended
            // recurring call to the daemon. See `stats_sample_interval` for why
            // it slows down rather than stopping.
            let stats_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    commands::sample_container_stats(&stats_handle).await;

                    // A window that cannot be asked is treated as visible. The
                    // failure mode of guessing the other way is a sparkline
                    // that quietly samples five times too slowly for the life
                    // of the process, which nothing would report.
                    let visible = stats_handle
                        .get_webview_window(MAIN_WINDOW)
                        .map(|w| {
                            w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false)
                        })
                        .unwrap_or(true);

                    tokio::time::sleep(commands::stats_sample_interval(visible)).await;
                }
            });

            Ok(())
        })
        // The app menu and the tray share one callback, so About is offered
        // the event first and the tray handles what is left.
        .on_menu_event(|app, event| {
            if !menu::handle_menu_event(app, &event) {
                tray::handle_menu_event(app, event);
            }
        })
        .on_window_event(|window, event| {
            // Only the main window. Both arms below act on state that belongs
            // to it — the close-behaviour flow and the shells it opened — and
            // both were running for every window: closing the About box asked
            // whether to stop the stack, and killed the user's terminals on
            // the way out.
            if window.label() != MAIN_WINDOW {
                return;
            }

            match event {
                // Closing the window used to end the process, which made the
                // tray pointless: the app built a glanceable status icon and
                // then exited the moment you stopped looking at the window.
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Every path prevents the default close. Even "quit" does,
                    // because exiting is then our decision to make after the
                    // stack has been dealt with, not something that happens
                    // while an async stop is still in flight.
                    api.prevent_close();

                    let behaviour = commands::close_behaviour();
                    if behaviour == commands::CLOSE_ASK {
                        // The front end owns the dialog: it has to offer
                        // "remember this", and a remembered choice is the same
                        // preference the Settings page edits.
                        events::emit(&window.app_handle().clone(), "app:close_requested", ());
                        return;
                    }

                    let handle = window.app_handle().clone();
                    tauri::async_runtime::spawn(commands::apply_close(handle, behaviour));
                }
                tauri::WindowEvent::Destroyed => {
                    // Shells must not outlive the window that opened them.
                    pty::close_all(&window.state::<pty::Registry>());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Phase 1 — reads
            commands::workspace_get,
            commands::workspace_set,
            commands::bootstrap_complete,
            commands::engine_status,
            commands::engine_start,
            commands::host_stats,
            commands::docker_system_resources,
            commands::docker_disk_usage,
            commands::projects_list,
            commands::services_list,
            commands::catalog_get,
            commands::preflight,
            commands::preflight_fix,
            commands::doctor,
            commands::server_config_get,
            commands::server_config_set,
            commands::hosts_missing_core,
            commands::templates_list,
            commands::template_override,
            commands::template_revert,
            commands::env_get,
            commands::env_defaults,
            commands::env_reveal,
            commands::tunnel_status,
            commands::worker_options,
            commands::worker_status,
            // Phase 2 — mutations
            commands::docker_prune,
            commands::mail_delete,
            commands::mail_search,
            commands::mail_html_check,
            commands::mail_link_check,
            commands::mail_attachment_save,
            commands::tunnel_start,
            commands::tunnel_stop,
            commands::worker_start,
            commands::worker_stop,
            commands::project_scaffold,
            commands::project_clone,
            commands::project_register,
            commands::git_available,
            commands::project_start,
            commands::project_stop,
            commands::project_restart,
            commands::project_build,
            commands::service_start,
            commands::service_stop,
            commands::service_restart,
            commands::service_settings,
            commands::service_apply_settings,
            commands::service_enable,
            commands::service_disable,
            commands::market_status,
            commands::market_refresh,
            commands::market_catalog,
            commands::market_install,
            commands::market_uninstall,
            commands::market_probe,
            commands::handover_preview,
            commands::handover_apply,
            commands::instance_list,
            commands::instance_create,
            commands::instance_remove,
            commands::instance_promote,
            commands::instance_enable,
            commands::instance_disable,
            commands::instance_start,
            commands::instance_stop,
            commands::instance_restart,
            commands::container_inspect,
            commands::container_stats,
            commands::container_logs_open,
            commands::container_logs_close,
            commands::app_logs,
            commands::app_log_open,
            commands::app_logs_all,
            commands::app_logs_all_open,
            commands::env_set,
            commands::generate_run,
            commands::compose_up,
            commands::compose_down,
            // Phase 3 — desktop integration
            commands::hosts_status,
            commands::hosts_plan,
            commands::hosts_apply,
            commands::hosts_missing,
            commands::hosts_overview,
            // Certificates — the trusted-HTTPS surface the Bash helper had and
            // the app could not see.
            commands::mail_status,
            commands::mail_messages,
            commands::mail_message,
            commands::mail_clear,
            commands::db_targets,
            commands::db_dump,
            commands::db_snapshots,
            commands::db_snapshot_take,
            commands::db_snapshot_restore,
            commands::db_snapshot_delete,
            commands::db_restore,
            commands::service_connection,
            commands::xdebug_status,
            commands::xdebug_set,
            commands::php_ini_status,
            commands::php_ini_set,
            commands::doctor_drop_extension,
            commands::debug_bridge_set,
            commands::debug_bridge_events,
            commands::debug_bridge_clear,
            commands::debug_bridge_overview,
            commands::release_plan,
            commands::release_build,
            commands::release_save,
            commands::release_load,
            commands::profiler_status,
            commands::profiler_set_mode,
            commands::profiler_read,
            commands::profiler_delete,
            commands::profiler_clear,
            commands::quick_commands,
            commands::quick_command_run,
            commands::devserver_status,
            commands::devserver_set,
            commands::migrate_scan,
            commands::migrate_apply,
            commands::preset_export,
            commands::preset_save,
            commands::preset_plan,
            commands::preset_apply,
            commands::cert_status,
            commands::cert_plan,
            commands::cert_apply,
            commands::pty_open,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_close,
            commands::cert_trust_in_terminal,
            commands::terminal_open_external,
            // Gap fill — declared in the contract from Phase 0, implemented now
            commands::workspace_pick,
            commands::project_get,
            commands::project_validate,
            commands::project_create,
            commands::project_delete,
            commands::imports_scan,
            commands::imports_scan_at,
            commands::imports_take,
            commands::project_adoptable,
            commands::project_adopt,
            commands::project_manifest_read,
            commands::project_manifest_write,
            commands::project_requirements,
            commands::project_requirements_apply,
            commands::project_requirements_declare,
            commands::service_dependencies,
            commands::container_stats_history,
            commands::containers_start_all,
            commands::containers_stop_all,
            commands::containers_restart_all,
            commands::compose_up_service,
            commands::compose_up_project,
            commands::compose_restart,
            commands::open_in_editor,
            commands::open_in_browser,
            commands::open_folder,
            commands::updater_status,
            commands::licences_notice,
            commands::policy_status,
            commands::secrets_status,
            commands::secret_move,
            commands::secret_restore,
            commands::agents_status,
            commands::agents_install,
            commands::agents_remove,
            commands::system_accent,
            commands::logs_info,
            commands::diagnostics_bundle,
            commands::locale_get,
            commands::tray_relabel,
            commands::window_close_action,
            commands::apps_available,
            commands::prefs_get,
            commands::prefs_set,
            commands::project_dockerfile_preview,
            commands::generator_verify,
            commands::generate_with,
        ])
        .run(tauri::generate_context!())
        .expect("error while running StackVo");
}
