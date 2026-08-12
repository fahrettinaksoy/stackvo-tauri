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
 * The counted labels carry their placeholders through untouched. `t()` is not
 * asked to interpolate because Rust is the one that knows the numbers; sending
 * `'Containers: {count}'` and letting `fill` substitute keeps the ordering
 * decision in the language file, where a language that puts the count last can
 * express it.
 *
 * @param {(key: string) => string} t — vue-i18n's translate, already bound to
 *   the active locale.
 * @returns {Record<string, string>} every key `tray.rs`'s `LABEL_KEYS` names.
 */
export function trayLabels(t) {
  return {
    checking: t('tray.checking'),
    show: t('tray.show'),
    quit: t('tray.quit'),
    engineDown: t('tray.engineDown'),
    engineUp: t('tray.engineUp'),
    noWorkspace: t('tray.noWorkspace'),
    noProjects: t('tray.noProjects'),

    // Counted — the placeholders survive to Rust deliberately.
    containers: t('tray.containers'),
    more: t('tray.more'),
    runningSummary: t('tray.runningSummary'),

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
