//! The third-party licence notice, compiled into the binary.
//!
//! Every permissive licence in this dependency graph — MIT, BSD, ISC,
//! Apache-2.0 — asks for the same thing: the copyright notice and the licence
//! text travel with the software. A `NOTICE.md` sitting in a source repository
//! has not met that. The person who received a `.dmg` never saw the repository,
//! and pointing them at one is the same answer as not shipping it.
//!
//! So the file is `include_str!`'d rather than added to the bundle's resources.
//! Both put the text on the user's disk; only one of them cannot be missing.
//! A resource is a path resolved at run time against a directory layout that
//! differs on each of the three platforms — and when it resolves to nothing,
//! the app is one that silently ships no notices, which is exactly the state
//! this module exists to end. Compiled in, it is either there or the build
//! failed.
//!
//! The cost is honest and small: the notice is ~85 KB of text, and it is the
//! same 85 KB the obligation is about.
//!
//! `NOTICE.md` is generated — `node tools/generate-notice.mjs` — from
//! `Cargo.lock` and `package-lock.json`, and `npm run notice:check` fails the
//! build when a shipped dependency is missing from it.

/// The notice, verbatim.
pub const NOTICE: &str = include_str!("../../NOTICE.md");

#[cfg(test)]
mod tests {
    use super::*;

    /// The obligation, as a test: the text is present, and it is the notice
    /// rather than whatever else could end up at that path.
    #[test]
    fn the_notice_is_compiled_in() {
        assert!(
            NOTICE.contains("# Third-party notices"),
            "NOTICE.md is not the generated notice"
        );
        assert!(
            NOTICE.len() > 10_000,
            "the notice is {} bytes — too short to be the inventory of a tree \
             with several hundred dependencies",
            NOTICE.len()
        );
    }

    /// The two halves of the graph are both represented.
    ///
    /// A generator that ran with a broken npm walk produced a notice listing
    /// 572 crates and thirteen packages, and it looked entirely plausible. The
    /// cheapest guard against that shape of failure is naming something from
    /// each half that has to be there.
    #[test]
    fn both_halves_of_the_dependency_graph_are_listed() {
        for expected in ["| tauri |", "| bollard |", "| vue |", "| vuetify |"] {
            assert!(
                NOTICE.contains(expected),
                "{expected} is not in the notice — a direct dependency is missing \
                 from it, which means the generator's view of what ships is wrong"
            );
        }
    }

    /// A licence with no text is a citation, not a notice.
    #[test]
    fn the_licence_texts_are_carried_not_just_named() {
        assert!(
            NOTICE.contains("Permission is hereby granted, free of charge"),
            "the MIT text is missing"
        );
        assert!(
            NOTICE.contains("Apache License"),
            "the Apache-2.0 text is missing"
        );
        assert!(
            NOTICE.contains("## Copyright holders"),
            "the copyright lines are missing, and they are the part of a \
             permissive licence that is not boilerplate"
        );
    }
}
