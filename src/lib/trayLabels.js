/**
 * The catalog the Rust tray and menu bar are drawn from.
 *
 * The tray lives in Rust, so its strings used to live there too: a
 * `match (key, turkish)` table in `tray.rs` holding both languages. That made
 * every one of them a second copy of a translation the front end already had,
 * and — the reason this exists — it made **a third language a change to a Rust
 * file**, which is not what "the app speaks N languages" is supposed to mean.
 *
 * So the direction is reversed. The front end owns the words, and sends them
 * across on boot and on every language change. `tray.rs` keeps its table as the
 * fallback for the only moment this cannot cover: the tray is created during
 * `setup()`, before the webview exists, so something has to be on it for the
 * first second.
 *
 * Composed rather than copied. Only strings with no home elsewhere come from
 * the `tray` block — the navigation entries are the same words as the sidebar's,
 * the engine words are the dashboard's, and the menu bar's links are the About
 * window's. Duplicating them here would be the mistake this file exists to
 * undo, one level down.
 *
 * The counted labels carry their placeholders through untouched. Rust is the
 * one that knows the numbers; sending `'Containers: {count}'` and letting
 * `fill` substitute keeps the ordering decision in the language file, where a
 * language that puts the count last can express it.
 *
 * ## Why [`keeping`] exists, and what it cost to find out
 *
 * "`t()` is not asked to interpolate" was written here and was not true. A
 * named placeholder with no matching parameter is not left alone by vue-i18n —
 * it is substituted with the **empty string**. So every label meant to reach
 * Rust with a hole in it arrived with the hole already filled in with nothing:
 *
 * ```text
 * tray.stopProject   '{name}: durdur'          → ': durdur'
 * tray.containers    'Konteynerler: {count}'   → 'Konteynerler: '
 * tray.runningSummary '{running}/{total} …'    → '/ proje çalışıyor'
 * ```
 *
 * Rust then did its half correctly and found nothing to replace, so the tray
 * showed a Start/stop submenu of rows reading `: başlat` and `: durdur` — five
 * projects, no names, in a menu whose whole job is to say which one. (That
 * submenu is gone — the verbs live under each project now, and only the counted
 * labels still travel with a hole in them — but the trap it fell into is the
 * same one any future placeholder would meet.) It shipped
 * because every test in `tray-labels.spec.js` passed `t` as `(key) => key`,
 * which returns the path and interpolates nothing. The catalogue was checked,
 * the boundary was checked, and the one thing in between was a translator
 * nobody had actually run.
 *
 * The fix is to hand each placeholder back its own name as the value, so the
 * substitution is an identity. That keeps the locale files ordinary — the
 * alternative was escaping every brace in both languages, which is a rule
 * whoever adds a third language would have to be told.
 *
 * @param {(key: string, params?: Record<string, string>) => string} t —
 *   vue-i18n's translate, already bound to the active locale.
 * @returns {Record<string, string>} every key `tray.rs`'s `LABEL_KEYS` names.
 */

/**
 * Translate, and leave the named placeholders standing for Rust to fill.
 *
 * `t('Start {name}', { name: '{name}' })` is an identity substitution: the
 * message is resolved in the user's language, and the hole comes out the far
 * side exactly as wide as it went in.
 */
const keeping =
  (t) =>
  (key, ...names) =>
    t(key, Object.fromEntries(names.map((name) => [name, `{${name}}`])));

export function trayLabels(t) {
  const through = keeping(t);
  return {
    checking: t('tray.checking'),
    show: t('tray.show'),
    quit: t('tray.quit'),
    engineDown: t('tray.engineDown'),
    engineUp: t('tray.engineUp'),
    noWorkspace: t('tray.noWorkspace'),
    noProjects: t('tray.noProjects'),

    // The rows inside a project's own submenu (M-8). No `{name}` in any of
    // them any more: the project is the row they hang under, so the verb is a
    // word rather than a sentence repeating what the menu already says.
    openProject: t('tray.openProject'),
    startProject: t('projectsView.menu.start'),
    stopProject: t('projectsView.menu.stop'),

    // Counted — the placeholders survive to Rust deliberately.
    containers: through('tray.containers', 'count'),
    more: through('tray.more', 'count'),
    runningSummary: through('tray.runningSummary', 'running', 'total'),

    // Shared with the sidebar.
    navProjects: t('nav.projects'),
    navMarket: t('nav.market'),
    navLogs: t('nav.logs'),
    navSettings: t('nav.settings'),

    // Shared with the dashboard.
    docker: t('system.docker'),
    running: t('system.running'),
    stopped: t('system.stopped'),

    // Shared with the About window.
    menuAbout: t('tray.menuAbout'),
    menuDocs: t('about.links.docs'),
    menuSource: t('about.links.source'),
    menuIssues: t('about.links.issues'),
  };
}
