//! System tray.
//!
//! A web dashboard can only tell you something while a browser tab is open on
//! it. The most common thing a developer wants from StackVo — "is my stack up?"
//! — is a glance, not a page visit. The tray answers it without the app being
//! in the foreground.

use crate::commands::AppState;
use crate::engine;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

pub const MENU_SHOW: &str = "show";
pub const MENU_STATUS: &str = "status";
pub const MENU_QUIT: &str = "quit";

/// The tray's own strings.
///
/// The rest of the app speaks two languages through vue-i18n, but the tray is
/// built in Rust and never saw any of it — "Open StackVo" and "Quit" were the
/// one surface that stayed English regardless of the user's choice. There are
/// six strings, so a table beats plumbing a translation framework into the
/// native side.
///
/// Kept deliberately parallel to `src/i18n/locales/*.js`: if a seventh string
/// appears here it belongs in both places, and `tray_strings_cover_every_key`
/// checks the two locales stay in step with each other.
fn tr(locale: &str, key: &str) -> String {
    let turkish = locale.starts_with("tr");
    match (key, turkish) {
        ("checking", true) => "Docker denetleniyor…",
        ("checking", false) => "Checking Docker…",
        ("show", true) => "StackVo'yu aç",
        ("show", false) => "Open StackVo",
        ("quit", true) => "Çık",
        ("quit", false) => "Quit",
        ("engineDown", true) => "Docker çalışmıyor",
        ("engineDown", false) => "Docker is not running",
        ("noWorkspace", true) => "StackVo dizini seçilmedi",
        ("noWorkspace", false) => "No StackVo directory selected",
        ("engineUp", true) => "Docker çalışıyor",
        ("engineUp", false) => "Docker running",
        _ => key,
    }
    .to_string()
}

/// `{running}/{total}` reads the same in both languages; only the noun differs.
fn running_summary(locale: &str, running: usize, total: usize) -> String {
    if locale.starts_with("tr") {
        format!("{running}/{total} proje çalışıyor")
    } else {
        format!("{running}/{total} projects running")
    }
}

/// The user's language, as the front end stored it.
///
/// Read from preferences on every use rather than cached: the tray refreshes on
/// a timer anyway, and a cache would need invalidating from the one place that
/// changes the setting.
pub fn locale() -> String {
    crate::commands::preferred_locale()
}

/// The menu, in the user's current language.
///
/// Rebuilt rather than mutated when the language changes: `TrayIcon` exposes no
/// getter for its menu, so there is nothing to reach into. Six items is cheap
/// to recreate, and one construction path means the two languages cannot drift
/// into different menu shapes.
fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let lang = locale();
    let status = MenuItem::with_id(app, MENU_STATUS, tr(&lang, "checking"), false, None::<&str>)?;
    let show = MenuItem::with_id(app, MENU_SHOW, tr(&lang, "show"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, tr(&lang, "quit"), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    Menu::with_items(app, &[&status, &separator, &show, &separator, &quit])
}

/// Build the tray and its menu. The status line is a disabled item used purely
/// as a label, refreshed by `refresh`.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .menu(&menu)
        // Left-click opens the window; without this the menu is the only way in
        // on Windows and Linux, which reads as broken.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id.as_ref() {
        MENU_SHOW => show_window(app),
        MENU_QUIT => app.exit(0),
        _ => {}
    }
}

/// Re-label the menu after the user changes language.
///
/// Without this the tray keeps whatever it was built with until the next
/// launch, which reads as the setting not having worked.
pub fn relabel<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Ok(menu) = build_menu(app) {
        // The status line comes back as "Checking Docker…" until the next
        // refresh tick, which is a second away and honest in the meantime.
        let _ = tray.set_menu(Some(menu));
    }
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Update the tray tooltip and status line from live state.
///
/// Called on a slow timer — this is glanceable status, not a dashboard, and
/// polling the engine hard from a background timer would be rude to the daemon.
pub async fn refresh(app: AppHandle) {
    let status = engine::status().await;
    let lang = locale();

    let summary = if !status.reachable {
        tr(&lang, "engineDown")
    } else {
        let state = app.state::<AppState>();
        let root = {
            let ws = state.workspace.lock().ok();
            ws.and_then(|w| w.require_root().ok())
        };

        match root {
            None => tr(&lang, "noWorkspace"),
            Some(root) => match crate::commands::list_projects(&root).await {
                Ok(projects) => {
                    let running = projects.iter().filter(|p| p.running).count();
                    running_summary(&lang, running, projects.len())
                }
                Err(_) => tr(&lang, "engineUp"),
            },
        }
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&format!("StackVo — {summary}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_differs_between_the_two_languages() {
        // A key that returns the same text for both is almost always one that
        // was added to the match with only one arm filled in.
        for key in [
            "checking",
            "show",
            "quit",
            "engineDown",
            "noWorkspace",
            "engineUp",
        ] {
            assert_ne!(
                tr("tr", key),
                tr("en", key),
                "{key} is not actually translated"
            );
            assert_ne!(tr("en", key), key, "{key} fell through to the fallback");
        }
    }

    #[test]
    fn an_unknown_locale_gets_english() {
        assert_eq!(tr("de", "quit"), "Quit");
        assert_eq!(tr("", "quit"), "Quit");
    }

    #[test]
    fn a_regional_turkish_tag_is_still_turkish() {
        // vue-i18n stores "tr", but a system-derived value can be "tr-TR".
        assert_eq!(tr("tr-TR", "quit"), tr("tr", "quit"));
    }

    #[test]
    fn the_summary_counts_are_placed_in_both_languages() {
        assert!(running_summary("tr", 2, 5).contains("2/5"));
        assert!(running_summary("en", 2, 5).contains("2/5"));
        assert_ne!(running_summary("tr", 2, 5), running_summary("en", 2, 5));
    }
}
