//! The Bash generator's template renderer, in Rust.
//!
//! ## Why this one function first
//!
//! Measured before choosing: the Bash generator writes **40 files** into
//! `generated/`, and the Rust port already reproduces **14 of them
//! byte-for-byte** — every project Dockerfile, `docker-compose.projects.yml`,
//! and both Traefik files. `readyToTakeOver` is already true for that surface.
//!
//! What is left splits into exactly two mechanisms, not twenty:
//!
//! | surface | files | how Bash makes it |
//! | --- | --- | --- |
//! | project `nginx.conf` / `supervisord.conf` | 22 | inline heredocs in `dockerfile/*.sh`, then a `sed` |
//! | `docker-compose.dynamic.yml` | 1 | **this renderer**, over 20 service templates |
//! | `configs/*` | 6 | **this renderer**, over 6 config templates |
//!
//! So porting this single function unlocks 27 of the 29 remaining files. The
//! heredocs are the pattern [`crate::generator`] already uses for Dockerfiles.
//!
//! Two things turned up while measuring, both worth recording. `render_template`
//! is called from exactly two places — `generators/compose.sh` and
//! `generators/config.sh`. And **`core/templates/servers/` is dead**: nothing in
//! `core/` or `tools/` reads it, because `dockerfile/nginx.sh` writes the config
//! inline with a `DOCUMENT_ROOT_PLACEHOLDER` and `sed`s it afterwards. Ten
//! template files that look load-bearing and are not.
//!
//! ## The grammar is not "mustache"
//!
//! It is a Perl one-liner in `core/cli/lib/template-processor.sh`, and its
//! oddity is the reason this is a port rather than a library call: **a variable
//! that does not match the prefix list is deliberately left unexpanded**, as
//! `${VAR}`, for `docker compose` to interpolate later. Substituting it here
//! would bake one machine's value into a file the compose spec expects to
//! resolve at run time.
//!
//! Three passes, in order, per line:
//!
//! 1. `{{ VAR }}` and `{{ VAR | default('x') }}` — prefixed: the value, else
//!    the default, else empty. Not prefixed: `${VAR}` or `${VAR:-x}`.
//! 2. `${VAR}` — prefixed: the value or empty. Not prefixed: untouched.
//! 3. `$VAR` — the same, and *not* when preceded by `{`.
//!
//! The order is load-bearing: pass 1 emits `${VAR}` for unprefixed names, and
//! pass 2 must then leave those alone. It does, because it only rewrites
//! prefixed ones — but reordering the passes would quietly change the output of
//! every service template.

use std::collections::BTreeMap;

/// Names the renderer will substitute. Anything else is left for compose.
///
/// Copied from the Perl regex verbatim rather than generalised: the list is the
/// contract between the templates and the two things that read them, and
/// "tidying" it would change which variables survive into the generated file.
pub const PREFIXES: [&str; 8] = [
    "STACKVO_",
    "SERVICE_",
    "DEFAULT_",
    "DOCKER_",
    "TLD_",
    "HOST_",
    "SSL_",
    "REDIRECT_",
];

pub fn is_substituted(name: &str) -> bool {
    PREFIXES.iter().any(|p| name.starts_with(p))
}

/// A name as the Perl's `[A-Z0-9_]+` reads it, starting at `bytes[i]`.
///
/// Returns the name and the index after it. Empty means no name, which the
/// callers treat as "not a placeholder" and copy through.
fn read_name(bytes: &[u8], i: usize) -> (String, usize) {
    let mut j = i;
    while j < bytes.len()
        && (bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit() || bytes[j] == b'_')
    {
        j += 1;
    }
    (String::from_utf8_lossy(&bytes[i..j]).into_owned(), j)
}

/// Pass 1: `{{ VAR }}` / `{{ VAR | default('x') }}`.
fn pass_braces(line: &str, env: &BTreeMap<String, String>) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some((name, default, end)) = read_placeholder(bytes, i) {
                if is_substituted(&name) {
                    // The value, then the default, then empty — the Perl's
                    // order, and the difference between an empty setting and an
                    // unset one.
                    out.push_str(
                        env.get(&name)
                            .map(String::as_str)
                            .or(default.as_deref())
                            .unwrap_or(""),
                    );
                } else {
                    // Left for `docker compose` to interpolate. Substituting it
                    // here would bake this machine's value into the file.
                    match &default {
                        Some(d) => out.push_str(&format!("${{{name}:-{d}}}")),
                        None => out.push_str(&format!("${{{name}}}")),
                    }
                }
                i = end;
                continue;
            }
        }
        i += copy_one_char(line, i, &mut out);
    }
    out
}

/// Copy the character starting at byte `i`, and say how many bytes it took.
///
/// This existed as `out.push(bytes[i] as char)` in both passes, and that is a
/// **Latin-1 decode**: `u8 as char` reads one byte as a code point, so the
/// em-dash `E2 80 94` became U+00E2 U+0080 U+0094 and `String::push` re-encoded
/// each as two bytes. Twice, because a line goes through both passes — the
/// three bytes came out as six and then as eight.
///
/// It stayed invisible for as long as every template was ASCII. The comment
/// block in the Mongo template is the first that is not, and the way it
/// surfaced is worth recording: the golden fixture *matched*, because both
/// sides of the comparison were the mangled output. A byte-for-byte test does
/// not notice a corruption that is upstream of it.
///
/// Values were never affected — those arrive through `push_str` — so this is
/// literal template text: comments today, and anything anyone writes into a
/// template tomorrow.
///
/// `i` stays on a character boundary because this is the only thing that
/// advances it outside a placeholder, which is what makes `line[i..]` safe.
fn copy_one_char(line: &str, i: usize, out: &mut String) -> usize {
    match line[i..].chars().next() {
        Some(ch) => {
            out.push(ch);
            ch.len_utf8()
        }
        None => 1,
    }
}

/// `{{ NAME }}` or `{{ NAME | default('x') }}` starting at `i`, and where it
/// ends. `None` when the braces do not open a well-formed placeholder, which is
/// copied through rather than guessed at.
fn read_placeholder(bytes: &[u8], i: usize) -> Option<(String, Option<String>, usize)> {
    let mut j = i + 2;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }

    let (name, after) = read_name(bytes, j);
    if name.is_empty() {
        return None;
    }
    j = after;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }

    let mut default = None;
    if j < bytes.len() && bytes[j] == b'|' {
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if !bytes[j..].starts_with(b"default(") {
            return None;
        }
        j += "default(".len();

        let quote = *bytes.get(j)?;
        if quote != b'\'' && quote != b'"' {
            return None;
        }
        j += 1;
        let start = j;
        // Non-greedy to the matching quote, as the Perl's `(.*?)\2`.
        while j < bytes.len() && bytes[j] != quote {
            j += 1;
        }
        if j >= bytes.len() {
            return None;
        }
        default = Some(String::from_utf8_lossy(&bytes[start..j]).into_owned());
        j += 1;
        if bytes.get(j) != Some(&b')') {
            return None;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
    }

    if !bytes[j..].starts_with(b"}}") {
        return None;
    }
    Some((name, default, j + 2))
}

/// Passes 2 and 3: `${VAR}` and a bare `$VAR`.
fn pass_dollars(line: &str, env: &BTreeMap<String, String>) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' {
            // `${VAR}` — only when the whole brace holds a bare name, so
            // `${VAR:-default}` is left alone exactly as the Perl leaves it.
            if bytes.get(i + 1) == Some(&b'{') {
                let (name, after) = read_name(bytes, i + 2);
                if !name.is_empty() && bytes.get(after) == Some(&b'}') {
                    if is_substituted(&name) {
                        out.push_str(env.get(&name).map(String::as_str).unwrap_or(""));
                    } else {
                        out.push_str(&format!("${{{name}}}"));
                    }
                    i = after + 1;
                    continue;
                }
            } else {
                // Bare `$VAR`, and never when preceded by `{` — the Perl's
                // negative lookbehind, which Rust's regex crate has no way to
                // express and this scanner gets for free.
                let preceded_by_brace = i > 0 && bytes[i - 1] == b'{';
                let (name, after) = read_name(bytes, i + 1);
                if !name.is_empty() && !preceded_by_brace {
                    if is_substituted(&name) {
                        out.push_str(env.get(&name).map(String::as_str).unwrap_or(""));
                    } else {
                        out.push_str(&format!("${name}"));
                    }
                    i = after;
                    continue;
                }
            }
        }
        i += copy_one_char(line, i, &mut out);
    }
    out
}

/// Render a template the way `render_template` does.
///
/// Line by line, because the Perl reads the file a line at a time and its
/// substitutions cannot span one.
pub fn render(text: &str, env: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        out.push_str(&pass_dollars(&pass_braces(body, env), env));
        out.push_str(newline);
    }
    out
}

/// The variables a render sees: `.env`, plus the four the shell computes.
///
/// `core/cli/lib/env-loader.sh` exports `STACKVO_ROOT`, `HOST_STACKVO_ROOT`,
/// `HOST_UID` and `HOST_GID` before any template is rendered. **None of them is
/// in `.env`**, and all four match the prefix list, so a renderer fed `.env`
/// alone silently substitutes empty strings — which is what the first byte
/// comparison against the real output caught: every volume mount came out as
/// `/generated/configs/mysql.cnf` instead of an absolute host path, and the
/// resulting compose file would have mounted the wrong directory rather than
/// failing.
///
/// The shell's own environment is deliberately *not* consulted beyond these
/// four. A value that happens to be exported in the developer's terminal is not
/// part of the project's configuration, and letting it win would make the
/// generated files depend on who ran the app.
pub fn variables(env: &crate::config::Env, root: &std::path::Path) -> BTreeMap<String, String> {
    let mut vars = env.raw().clone();
    let root = root.display().to_string();

    // `${HOST_STACKVO_ROOT:-$STACKVO_ROOT}` — the override exists for running
    // the CLI inside a container, where the paths a bind mount needs are the
    // host's, not the container's. From the desktop app there is no such
    // indirection, but an override already in `.env` still wins.
    vars.entry("STACKVO_ROOT".into())
        .or_insert_with(|| root.clone());
    vars.entry("HOST_STACKVO_ROOT".into()).or_insert(root);

    #[cfg(unix)]
    {
        // `id -u` / `id -g`, without spawning `id`.
        vars.entry("HOST_UID".into())
            .or_insert_with(|| unsafe { libc_getuid() }.to_string());
        vars.entry("HOST_GID".into())
            .or_insert_with(|| unsafe { libc_getgid() }.to_string());
    }

    vars
}

#[cfg(unix)]
unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

#[cfg(unix)]
unsafe fn libc_getgid() -> u32 {
    extern "C" {
        fn getgid() -> u32;
    }
    getgid()
}

/// The `awk` filter `include_module` pipes every rendered service template
/// through, before appending it to `docker-compose.dynamic.yml`.
///
//
#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> BTreeMap<String, String> {
        [
            ("SERVICE_MYSQL_VERSION", "8.0"),
            ("DEFAULT_TLD_SUFFIX", "stackvo.loc"),
            ("SERVICE_REDIS_PASSWORD", ""),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    /// A template that is not ASCII survives both passes unchanged.
    ///
    /// `out.push(bytes[i] as char)` decoded each byte as Latin-1 and re-encoded
    /// it, so an em-dash grew from three bytes to six in one pass and to eight
    /// across both. Nothing caught it for as long as every template was ASCII,
    /// and the golden fixture could not: it froze the mangled bytes, so the
    /// comparison agreed with itself.
    ///
    /// Asserted on the bytes rather than on equality of `&str`, because a
    /// mangled string still prints as something and a reviewer reading a diff
    /// of `—` against `â€"` learns less than a reviewer reading a length.
    #[test]
    fn a_template_that_is_not_ascii_renders_unchanged() {
        for text in [
            "# hangs — host tools (Compass, TablePlus)",
            "# 中文 · Türkçe ıİşĞ · emoji 🎉",
            "MYSQL_DATABASE: \"kütüphane\"",
        ] {
            let rendered = render(text, &env());
            assert_eq!(
                rendered.as_bytes(),
                text.as_bytes(),
                "render changed non-ASCII text: {} bytes in, {} out",
                text.len(),
                rendered.len()
            );
        }
    }

    /// And it still substitutes when the placeholder sits next to one.
    #[test]
    fn a_placeholder_beside_non_ascii_still_resolves() {
        assert_eq!(
            render(
                "image: \"mysql:{{ SERVICE_MYSQL_VERSION }}\" # sürüm — sabit",
                &env()
            ),
            "image: \"mysql:8.0\" # sürüm — sabit"
        );
    }

    #[test]
    fn a_prefixed_placeholder_takes_its_value() {
        assert_eq!(
            render("image: mysql:{{ SERVICE_MYSQL_VERSION }}\n", &env()),
            "image: mysql:8.0\n"
        );
        // No spaces is the same placeholder.
        assert_eq!(render("{{SERVICE_MYSQL_VERSION}}", &env()), "8.0");
    }

    /// The oddity that makes this a port rather than a library call: a name
    /// outside the prefix list is left for `docker compose` to interpolate.
    /// Substituting it here would bake one machine's value into a file the
    /// compose spec expects to resolve at run time.
    #[test]
    fn an_unprefixed_placeholder_is_left_for_compose() {
        assert_eq!(render("{{ MY_APP_KEY }}", &env()), "${MY_APP_KEY}");
        assert_eq!(
            render("{{ MY_APP_KEY | default('abc') }}", &env()),
            "${MY_APP_KEY:-abc}"
        );
    }

    /// Value, then default, then empty — and an empty *setting* is a value.
    /// Falling through to the default there would silently re-enable a password
    /// somebody deliberately blanked.
    #[test]
    fn an_empty_value_beats_the_default() {
        assert_eq!(
            render("{{ SERVICE_REDIS_PASSWORD | default('secret') }}", &env()),
            ""
        );
        assert_eq!(
            render("{{ SERVICE_ABSENT_THING | default('fallback') }}", &env()),
            "fallback"
        );
        // Prefixed, absent, no default: empty, not the literal placeholder.
        assert_eq!(render("[{{ SERVICE_NOPE }}]", &env()), "[]");
    }

    #[test]
    fn both_quote_styles_parse() {
        assert_eq!(render("{{ X | default('a') }}", &env()), "${X:-a}");
        assert_eq!(render("{{ X | default(\"a\") }}", &env()), "${X:-a}");
    }

    #[test]
    fn dollar_forms_follow_the_same_prefix_rule() {
        assert_eq!(render("${DEFAULT_TLD_SUFFIX}", &env()), "stackvo.loc");
        assert_eq!(render("$DEFAULT_TLD_SUFFIX", &env()), "stackvo.loc");
        // Not prefixed: untouched, both forms.
        assert_eq!(render("${APP_ENV}", &env()), "${APP_ENV}");
        assert_eq!(render("$APP_ENV", &env()), "$APP_ENV");
    }

    /// `${VAR:-default}` is compose's own syntax and must survive intact — the
    /// Perl's `\$\{([A-Z0-9_]+)\}` cannot match it because of the `:-`.
    #[test]
    fn compose_default_syntax_is_not_eaten() {
        assert_eq!(
            render("${SERVICE_MYSQL_VERSION:-5.7}", &env()),
            "${SERVICE_MYSQL_VERSION:-5.7}"
        );
    }

    /// Pass 1 emits `${VAR}` for unprefixed names and pass 2 must leave those
    /// alone. Reordering the passes would quietly change every service template.
    #[test]
    fn the_output_of_the_first_pass_is_not_re_expanded() {
        // `{{ SERVICE_X }}` where SERVICE_X is unset renders empty, and the
        // empty string must not then be re-scanned into something.
        assert_eq!(render("a{{ MY_VAR }}b", &env()), "a${MY_VAR}b");
    }

    /// The Perl's negative lookbehind, which Rust's regex crate cannot express
    /// and this hand scanner gets for free.
    #[test]
    fn a_dollar_directly_after_a_brace_is_left_alone() {
        assert_eq!(
            render("{$DEFAULT_TLD_SUFFIX}", &env()),
            "{$DEFAULT_TLD_SUFFIX}"
        );
    }

    #[test]
    fn malformed_braces_are_copied_through_rather_than_guessed_at() {
        for input in ["{{ }}", "{{ lowercase }}", "{{ X | upper() }}", "{{ X"] {
            assert_eq!(render(input, &env()), input, "{input}");
        }
    }

    /// Line-oriented, like the Perl, and the file's own line endings survive.
    #[test]
    fn line_structure_is_preserved_exactly() {
        assert_eq!(render("a\nb\n", &env()), "a\nb\n");
        assert_eq!(render("a\nb", &env()), "a\nb");
        assert_eq!(render("", &env()), "");
        assert_eq!(render("\n\n", &env()), "\n\n");
    }
}
