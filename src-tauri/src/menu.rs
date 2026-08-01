//! The application menu bar.
//!
//! Tauri builds a sensible default — the app menu, Edit with the clipboard
//! shortcuts, Window, Help — and that default is kept rather than replaced.
//! Replacing it means re-declaring every standard item by hand, and the first
//! one forgotten is the one nobody tests: `Cmd+V` stops working in a text
//! field and nothing in the app looks broken.
//!
//! So this only *adds*. `Menu::default` gives the Help submenu a stable id, so
//! the item can be appended to it without matching on a title the platform may
//! have translated.

use crate::error::Result;
use tauri::menu::{
    Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const MENU_ABOUT: &str = "stackvo:about";

/// The app menu's copy of the same item.
///
/// Its own id rather than the same string twice: two menu items sharing one id
/// is a thing that may work, and "may" is not a property to build a menu bar
/// on. Both are handled, so both open the same window.
pub const MENU_ABOUT_APP: &str = "stackvo:about-app";

/// The links, and where each goes.
///
/// The same three the About window offers. Duplicated as a menu rather than
/// only living there because a menu bar is where a desktop app is looked in
/// for them, and the About window is one click further away than the thing it
/// would be reached through.
const LINKS: [(&str, &str); 3] = [
    ("stackvo:docs", "https://stackvo.github.io/stackvo"),
    ("stackvo:source", "https://github.com/stackvo/stackvo"),
    (
        "stackvo:issues",
        "https://github.com/stackvo/stackvo/issues",
    ),
];

/// The window the About item opens. Its own label, so the frontend can tell it
/// apart from the main window and render the card alone rather than the shell.
const ABOUT_LABEL: &str = "about";

/// The default menu, with "About StackVo" added to Help.
///
/// About lives in the app menu on macOS by convention and Tauri's default puts
/// it there. This is the second way to reach it, asked for because that is
/// where people looked — and a second route to the same screen costs nothing,
/// while a screen nobody can find costs a support message.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    labels: &Labels,
    product: &str,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    let links = [
        MenuItem::with_id(app, LINKS[0].0, &labels.docs, true, None::<&str>)?,
        MenuItem::with_id(app, LINKS[1].0, &labels.source, true, None::<&str>)?,
        MenuItem::with_id(app, LINKS[2].0, &labels.issues, true, None::<&str>)?,
    ];
    let about = MenuItem::with_id(app, MENU_ABOUT, &labels.about, true, None::<&str>)?;

    // The app menu's own About is a predefined item, which opens the native
    // panel — a different screen from the one Help opens, for the same words.
    // Two answers to one question is worse than either answer, so its first
    // item is replaced with the same item Help gets.
    //
    // The submenu is rebuilt rather than edited: `Menu::default` titles it with
    // the crate name, so a `stackvo-desktop` sat in the menu bar of an app
    // called StackVo. Its contents are the eight standard macOS entries, all
    // predefined, so reproducing them costs no behaviour.
    #[cfg(target_os = "macos")]
    {
        let app_about = MenuItem::with_id(app, MENU_ABOUT_APP, &labels.about, true, None::<&str>)?;
        let replacement = Submenu::with_items(
            app,
            product,
            true,
            &[
                &app_about,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;

        if let Some(first) = menu.items()?.first() {
            menu.remove(first)?;
        }
        menu.insert(&replacement, 0)?;
    }

    match menu.get(HELP_SUBMENU_ID) {
        Some(MenuItemKind::Submenu(help)) => {
            for item in &links {
                help.append(item)?;
            }
            help.append(&PredefinedMenuItem::separator(app)?)?;
            help.append(&about)?;
        }
        // A platform whose default menu has no Help submenu still gets the
        // items, at the top level, rather than silently getting nothing.
        _ => {
            for item in &links {
                menu.append(item)?;
            }
            menu.append(&about)?;
        }
    }

    Ok(menu)
}

/// The menu's text, resolved by the caller.
///
/// Passed in rather than looked up here: the translations live in the
/// frontend's locale files, and a second copy in Rust is a second thing to
/// keep in step.
pub struct Labels {
    pub about: String,
    pub docs: String,
    pub source: String,
    pub issues: String,
}

/// Open the About window, or focus it if it is already up.
///
/// A second window rather than a dialog in the main one: it is reachable from
/// the menu bar with no main window in focus, which is exactly when somebody
/// checks a version.
pub fn open_about<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if let Some(window) = app.get_webview_window(ABOUT_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        ABOUT_LABEL,
        WebviewUrl::App("index.html#/about".into()),
    )
    .title("")
    .inner_size(420.0, 560.0)
    .resizable(false)
    .minimizable(false)
    .maximizable(false)
    .build()
    .map_err(|e| {
        crate::error::Error::new(
            crate::error::Code::IoError,
            format!("opening the about window: {e}"),
        )
    })?;

    let _ = window.set_focus();
    Ok(())
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: &MenuEvent) -> bool {
    let id = event.id.as_ref();

    if id == MENU_ABOUT || id == MENU_ABOUT_APP {
        if let Err(e) = open_about(app) {
            tracing::warn!(error = %e.message, "could not open the about window");
        }
        return true;
    }

    if let Some((_, url)) = LINKS.iter().find(|(item, _)| *item == id) {
        // The app's own command, not the opener plugin: it resolves the
        // browser chosen in Preferences, so a link from the menu bar lands
        // where a link from a project card does.
        if let Err(e) = crate::commands::open_in_browser((*url).to_string()) {
            tracing::warn!(url = url, error = %e.message, "could not open a menu link");
        }
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_about_item_has_its_own_id() {
        // The tray menu handles its own ids in the same callback, so a clash
        // would route a tray click into the About window or the reverse.
        assert_ne!(MENU_ABOUT, crate::tray::MENU_SHOW);
        assert_ne!(MENU_ABOUT, crate::tray::MENU_QUIT);
        assert!(MENU_ABOUT.starts_with("stackvo:"));

        // The window event handler acts only on the main window: the close
        // flow and the shells belong to it. Give the About box that label and
        // closing it would ask whether to stop the stack, and take the user's
        // terminals with it.
        assert_ne!(ABOUT_LABEL, crate::MAIN_WINDOW);

        // Every link has a distinct id and an http(s) URL — `open_in_browser`
        // refuses anything else, so a typo here would be a menu item that
        // silently does nothing.
        let mut ids: Vec<&str> = LINKS.iter().map(|(id, _)| *id).collect();
        ids.push(MENU_ABOUT);
        ids.push(MENU_ABOUT_APP);
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "two menu items share an id");
        assert!(LINKS.iter().all(|(_, url)| url.starts_with("https://")));
    }
}
