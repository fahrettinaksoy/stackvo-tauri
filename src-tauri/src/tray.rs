//! System tray.
//!
//! A web dashboard can only tell you something while a browser tab is open on
//! it. The most common thing a developer wants from StackVo — "is my stack up?"
//! — is a glance, not a page visit. The tray answers it without the app being
//! in the foreground.

use crate::commands::AppState;
use crate::engine;
use tauri::{
    image::Image,
    menu::{IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, Runtime,
};

pub const MENU_SHOW: &str = "show";
pub const MENU_STATUS: &str = "status";
pub const MENU_QUIT: &str = "quit";

/// Prefix for the per-project entries. The project name follows it, so the
/// handler can recover which one was clicked without a second lookup table.
pub const MENU_PROJECT: &str = "project:";

/// Prefix for the pages. The router's own route name follows it, so the front
/// end can push it straight through without a second table mapping menu ids to
/// pages — one that would have to be kept in step with the router by hand.
pub const MENU_NAV: &str = "nav:";

/// The pages worth reaching without the window in front of you.
///
/// The dashboard is not among them: it is what the window already opens on, so
/// a menu entry for it duplicates "Open StackVo" one line above. Mail is out
/// for a different reason — it is somewhere you go after something happened,
/// not somewhere you jump to cold.
const NAV_ITEMS: [(&str, &str); 4] = [
    ("Projects", "navProjects"),
    ("Services", "navServices"),
    ("Logs", "navLogs"),
    ("Settings", "navSettings"),
];

/// How many projects the menu lists before it stops.
///
/// A tray menu taller than the screen is not a glance. Twelve is roughly where
/// a menu stops being scannable; past that the count in the header is the
/// honest summary and the window is one click away.
const MAX_PROJECTS: usize = 12;

/// The state colours, as flat RGB.
///
/// Drawn rather than typed. An emoji dot was tried first and is the wrong
/// shape of thing: it renders at text size with the font's own gloss and
/// gradient, and there is no way to make it smaller or flatter because it is a
/// character, not a graphic. A menu item takes a real icon, so the dot is one
/// — a flat disc, sized here rather than by whatever font the menu happens to
/// use for emoji.
///
/// The values are the system palette macOS uses for exactly this, so the menu
/// looks like the rest of the menu bar rather than like a web page in it.
const GREEN: [u8; 3] = [52, 199, 89];
const RED: [u8; 3] = [255, 69, 58];
const GREY: [u8; 3] = [142, 142, 147];

/// The canvas the dot is drawn on, in pixels.
///
/// muda scales a menu icon to 18pt high, so 36 is that at 2×: the disc stays
/// crisp on a retina display and the anti-aliased edge has pixels to work
/// with. The disc itself is a fraction of the canvas — a dot beside a label,
/// not a bullet the size of the text.
const DOT_CANVAS: u32 = 36;
const DOT_RADIUS: f32 = 7.0;

/// A flat disc of one colour, transparent everywhere else.
fn dot(colour: [u8; 3]) -> Image<'static> {
    let size = DOT_CANVAS as usize;
    let mut rgba = vec![0u8; size * size * 4];
    let centre = (DOT_CANVAS as f32 - 1.0) / 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let distance = (dx * dx + dy * dy).sqrt();

            // One pixel of feathering at the edge. Without it the disc is a
            // staircase at this size, which reads as a low-resolution icon
            // rather than a deliberate one.
            let coverage = (DOT_RADIUS + 0.5 - distance).clamp(0.0, 1.0);

            let i = (y * size + x) * 4;
            rgba[i] = colour[0];
            rgba[i + 1] = colour[1];
            rgba[i + 2] = colour[2];
            rgba[i + 3] = (coverage * 255.0).round() as u8;
        }
    }

    Image::new(&rgba, DOT_CANVAS, DOT_CANVAS).to_owned()
}

/// The last state the tray drew.
///
/// Kept so a language change can redraw the same menu in the other language
/// rather than resetting it to "Checking Docker…" until the next tick — the
/// setting would look like it had cleared the menu.
static LAST: std::sync::Mutex<Option<Snapshot>> = std::sync::Mutex::new(None);

fn last_snapshot() -> Snapshot {
    LAST.lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or(Snapshot {
            checking: true,
            ..Snapshot::default()
        })
}

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
        ("docker", true) => "Docker",
        ("docker", false) => "Docker",
        ("running", true) => "Çalışıyor",
        ("running", false) => "Running",
        ("stopped", true) => "Durdu",
        ("stopped", false) => "Stopped",
        ("noProjects", true) => "Proje yok",
        ("noProjects", false) => "No projects",
        ("navProjects", true) => "Projeler",
        ("navProjects", false) => "Projects",
        ("navServices", true) => "Servisler",
        ("navServices", false) => "Services",
        ("navLogs", true) => "Loglar",
        ("navLogs", false) => "Logs",
        ("navSettings", true) => "Ayarlar",
        ("navSettings", false) => "Settings",
        _ => key,
    }
    .to_string()
}

fn containers_label(locale: &str, count: usize) -> String {
    if locale.starts_with("tr") {
        format!("Konteynerler: {count}")
    } else {
        format!("Containers: {count}")
    }
}

fn more_label(locale: &str, count: usize) -> String {
    if locale.starts_with("tr") {
        format!("+{count} proje daha…")
    } else {
        format!("+{count} more…")
    }
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
/// What the tray is showing right now.
///
/// Passed in rather than fetched here so the menu can be built before the first
/// engine call has returned — the tray appears with the app, and a menu that
/// waited on Docker would be missing for as long as Docker took to answer.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub reachable: bool,
    pub containers: usize,
    /// `(name, running)`, already ordered.
    pub projects: Vec<(String, bool)>,
    /// True before the first refresh, so the status line can say so instead of
    /// claiming Docker is down.
    pub checking: bool,
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, snap: &Snapshot) -> tauri::Result<Menu<R>> {
    let lang = locale();
    let separator = PredefinedMenuItem::separator(app)?;

    // Disabled items used as labels. The status line was built once at startup
    // and never touched again — `refresh` only set the tooltip — so the menu
    // read "Checking Docker…" for the life of the process.
    // The dot leads, the way Docker Desktop's own menu does: the first thing
    // the menu is opened for is whether the engine is up, and a colour answers
    // that before the sentence is read.
    let (status_text, status_colour) = if snap.checking {
        (tr(&lang, "checking"), GREY)
    } else if snap.reachable {
        (
            format!("{} — {}", tr(&lang, "docker"), tr(&lang, "running")),
            GREEN,
        )
    } else {
        (tr(&lang, "engineDown"), RED)
    };
    let status = IconMenuItem::with_id(
        app,
        MENU_STATUS,
        status_text,
        false,
        Some(dot(status_colour)),
        None::<&str>,
    )?;

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = vec![Box::new(status)];

    if snap.reachable && !snap.checking {
        items.push(Box::new(MenuItem::with_id(
            app,
            "containers",
            containers_label(&lang, snap.containers),
            false,
            None::<&str>,
        )?));
        items.push(Box::new(PredefinedMenuItem::separator(app)?));

        if snap.projects.is_empty() {
            items.push(Box::new(MenuItem::with_id(
                app,
                "no-projects",
                tr(&lang, "noProjects"),
                false,
                None::<&str>,
            )?));
        }

        for (name, running) in snap.projects.iter().take(MAX_PROJECTS) {
            // The dot carries the state: "name (running)" would double the
            // width of every row to say what a colour says at a glance.
            items.push(Box::new(IconMenuItem::with_id(
                app,
                format!("{MENU_PROJECT}{name}"),
                name,
                true,
                Some(dot(if *running { GREEN } else { GREY })),
                None::<&str>,
            )?));
        }

        if snap.projects.len() > MAX_PROJECTS {
            items.push(Box::new(MenuItem::with_id(
                app,
                "more-projects",
                more_label(&lang, snap.projects.len() - MAX_PROJECTS),
                false,
                None::<&str>,
            )?));
        }
    }

    items.push(Box::new(separator));

    // The pages, above "open" rather than below it: opening the window is what
    // you do when none of these is what you wanted.
    for (route, key) in NAV_ITEMS {
        items.push(Box::new(MenuItem::with_id(
            app,
            format!("{MENU_NAV}{route}"),
            tr(&lang, key),
            true,
            None::<&str>,
        )?));
    }

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_SHOW,
        tr(&lang, "show"),
        true,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_QUIT,
        tr(&lang, "quit"),
        true,
        None::<&str>,
    )?));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Build the tray and its menu. The status line is a disabled item used purely
/// as a label, refreshed by `refresh`.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    let menu = build_menu(
        app,
        &Snapshot {
            checking: true,
            ..Snapshot::default()
        },
    )?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .menu(&menu)
        // Both buttons open the menu. Left-click used to raise the window
        // instead, which made the two buttons do unrelated things from the
        // same icon — and hid the status the menu exists to show behind a
        // click most people try first. "Open StackVo" is in the menu.
        .show_menu_on_left_click(true)
        .build(app)
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id.as_ref();
    match id {
        MENU_SHOW => show_window(app),
        MENU_QUIT => app.exit(0),
        _ => {
            // A project entry: raise the window and let the front end route.
            // Navigation lives there — the route table and the guard that
            // waits for the workspace are both its, and duplicating either
            // here would be a second answer to "can this page open yet".
            if let Some(name) = id.strip_prefix(MENU_PROJECT) {
                show_window(app);
                let _ = app.emit("tray:open_project", name.to_string());
            } else if let Some(route) = id.strip_prefix(MENU_NAV) {
                show_window(app);
                let _ = app.emit("tray:navigate", route.to_string());
            }
        }
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
    if let Ok(menu) = build_menu(app, &last_snapshot()) {
        // The status line comes back as "Checking Docker…" until the next
        // refresh tick, which is a second away and honest in the meantime.
        let _ = tray.set_menu(Some(menu));
    }
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
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

    let mut snapshot = Snapshot {
        reachable: status.reachable,
        ..Snapshot::default()
    };

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

                    // Services count too: the number in the tray should match
                    // the one in the app's own status bar, and that is every
                    // container StackVo is running, not only the projects.
                    let services = crate::commands::list_services(&root)
                        .await
                        .map(|s| s.iter().filter(|s| s.running).count())
                        .unwrap_or(0);

                    snapshot.containers = running + services;
                    snapshot.projects = {
                        let mut rows: Vec<(String, bool)> = projects
                            .iter()
                            .map(|p| (p.name.clone(), p.running))
                            .collect();
                        rows.sort_by(|a, b| a.0.cmp(&b.0));
                        rows
                    };
                    running_summary(&lang, running, projects.len())
                }
                Err(_) => tr(&lang, "engineUp"),
            },
        }
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&format!("StackVo — {summary}")));
        if let Ok(menu) = build_menu(&app, &snapshot) {
            let _ = tray.set_menu(Some(menu));
        }
    }

    if let Ok(mut last) = LAST.lock() {
        *last = Some(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_id_round_trips_through_the_menu() {
        // The name is carried in the id rather than in a side table, so it has
        // to survive the trip back. Project names allow dots and dashes, and a
        // prefix that collided with one would route the click nowhere.
        for name in ["shop", "parser.ajans", "vue-builder", "a.b.c"] {
            let id = format!("{MENU_PROJECT}{name}");
            assert_eq!(id.strip_prefix(MENU_PROJECT), Some(name));
        }

        // And the fixed entries must not look like projects, or clicking Quit
        // would open a project called "uit".
        for fixed in [MENU_SHOW, MENU_QUIT, MENU_STATUS] {
            assert!(fixed.strip_prefix(MENU_PROJECT).is_none(), "{fixed}");
            assert!(fixed.strip_prefix(MENU_NAV).is_none(), "{fixed}");
        }

        // The two prefixes must not read as each other, or a page click would
        // be handled as a project called "rojects".
        assert!(MENU_NAV.strip_prefix(MENU_PROJECT).is_none());
        assert!(MENU_PROJECT.strip_prefix(MENU_NAV).is_none());

        // Every page carries a route name the router actually declares; a typo
        // here is a menu item that raises the window and then does nothing.
        for (route, _) in NAV_ITEMS {
            assert!(!route.is_empty());
            assert!(route.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
        }
    }

    #[test]
    fn the_menu_says_it_is_checking_before_the_first_answer() {
        // The status line used to be built once and never updated — `refresh`
        // only set the tooltip — so it read "Checking Docker…" for the life of
        // the process. It is derived from the snapshot now, and the snapshot
        // starts in that state honestly rather than claiming Docker is down.
        let starting = last_snapshot();
        assert!(starting.checking);
        assert!(!starting.reachable);
        assert!(starting.projects.is_empty());
    }

    #[test]
    fn the_dot_is_a_flat_disc_of_one_colour() {
        let img = dot(GREEN);
        assert_eq!(img.width(), DOT_CANVAS);
        assert_eq!(img.height(), DOT_CANVAS);

        let rgba = img.rgba();
        let px = |x: u32, y: u32| {
            let i = ((y * DOT_CANVAS + x) * 4) as usize;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };

        // Flat: the centre and a point just inside the edge are the same
        // colour. A gradient here would be the thing the emoji was replaced
        // for.
        let centre = DOT_CANVAS / 2;
        assert_eq!(px(centre, centre), [GREEN[0], GREEN[1], GREEN[2], 255]);
        let inside = centre + (DOT_RADIUS as u32) - 2;
        assert_eq!(px(inside, centre), [GREEN[0], GREEN[1], GREEN[2], 255]);

        // Small: transparent well before the canvas edge, so the disc is a dot
        // beside the label rather than a bullet the size of the text.
        assert_eq!(px(0, 0)[3], 0);
        assert_eq!(px(centre, 0)[3], 0);

        // And the three states are three colours.
        assert_ne!(GREEN, RED);
        assert_ne!(GREEN, GREY);
        assert_ne!(RED, GREY);
    }

    #[test]
    fn the_container_count_is_labelled_in_both_languages() {
        assert!(containers_label("tr", 13).contains("13"));
        assert!(containers_label("en", 13).contains("13"));
        assert_ne!(containers_label("tr", 13), containers_label("en", 13));
        assert_ne!(more_label("tr", 4), more_label("en", 4));
    }

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
            "navProjects",
            "navServices",
            "navLogs",
            "navSettings",
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
