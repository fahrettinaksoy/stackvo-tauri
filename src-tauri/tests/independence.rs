//! The app must work with no StackVo checkout anywhere.
//!
//! Everything it needs — templates, `.env.example`, the directory layout — is
//! compiled into the binary, so pointing it at an empty folder has to produce
//! a workspace that generates. This was verified by hand while the skeleton
//! was being embedded; it is a test now because the failure mode is silent.
//! A missing template does not crash, it just renders a shorter file, and the
//! only way to notice is to check that the output is actually complete.

use stackvo_desktop_lib::{commands, skeleton, workspace};

/// Installs into a fresh temp directory and renders. No `STACKVO_ROOT`, no
/// sibling checkout, nothing on disk but what `install` put there.
#[test]
fn an_empty_folder_becomes_a_working_workspace() {
    let dir = std::env::temp_dir().join(format!(
        "stackvo-independence-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    assert_eq!(
        skeleton::fitness(&dir),
        skeleton::Fitness::Installable,
        "an empty folder should be installable"
    );
    skeleton::install(&dir).expect("install");

    // The directories the generator writes into. If any of these is missing the
    // app is still depending on a checkout it no longer ships with.
    for required in ["generated", "logs"] {
        assert!(
            dir.join(required).exists(),
            "{required} missing from a freshly installed app directory"
        );
    }

    // `projects` is the user's and install does not create it: its existence
    // would be an answer to a question only the user can answer, and the gate
    // would read that answer as "ready".
    assert!(!dir.join("projects").exists());

    // Rendering needs one, so the test names it — an empty directory, because
    // what is being proved is that everything below comes out of the binary
    // rather than out of a project tree.
    let projects = dir.join("code");
    std::fs::create_dir_all(&projects).expect("projects");
    workspace::point_at_projects(&dir, &projects).expect("pointer");

    // And `core/` is NOT one of them. This used to require
    // `core/templates/services` on disk, which made the rest of this test a
    // weaker claim than it looked: it proved the render worked with the
    // templates sitting right there, not that it worked from the binary. With
    // no copy to fall back on, everything below is now a statement about what
    // is compiled in.
    assert!(
        !dir.join("core").exists(),
        "installing copied templates that should have stayed in the binary"
    );

    // And no settings file. The whole point of the embedded defaults is that
    // there is nothing to copy into a new workspace; a `.env` here would mean
    // something was being shipped again.
    assert!(
        !dir.join(".env").exists(),
        "a fresh workspace should carry no overrides"
    );

    let (files, skipped) = commands::render_generated(&dir).expect("render");
    assert!(
        skipped.is_empty(),
        "a fresh workspace should have nothing to skip, got {skipped:?}"
    );
    assert!(
        files.len() >= 10,
        "expected a full render, got {} file(s)",
        files.len()
    );

    // Nothing ships switched on, so a fresh workspace assembles no services at
    // all. Mailpit was always the example of this; it is now the rule.
    //
    // The file still has to be one Compose accepts, and that is the part with
    // teeth: `services:` with nothing under it is null rather than an empty
    // mapping, and Compose rejects the whole merged set with "services must be
    // a mapping" — Traefik included.
    let compose = files
        .iter()
        .find(|f| f.path.ends_with("docker-compose.dynamic.yml"))
        .expect("docker-compose.dynamic.yml should be rendered");
    assert!(
        compose.content.starts_with("services: {}"),
        "an empty service set must be an empty mapping, not a null key:\n{}",
        compose.content
    );
    for service in ["mysql", "redis", "mailpit"] {
        assert!(
            !compose.content.contains(&format!("\n  {service}:\n")),
            "{service} should not be running before anyone asked for it"
        );
    }

    // Same trap, and this one is on the first-run path rather than behind a
    // settings change: a workspace with no projects yet.
    let projects = files
        .iter()
        .find(|f| f.path.ends_with("docker-compose.projects.yml"))
        .expect("docker-compose.projects.yml should be rendered");
    assert!(
        projects.content.contains("services: {}"),
        "a project-less compose file must be valid too:\n{}",
        projects.content
    );

    // The templates still all resolve from the binary, which is what this file
    // is about — switched off is not the same as missing, and the proof is
    // that the volumes they declare are still harvested.
    for volume in ["stackvo-mysql-data", "stackvo-redis-data"] {
        assert!(
            compose.content.contains(volume),
            "{volume} missing — a template did not resolve from the binary"
        );
    }

    // The retired web UI must not come back, whatever the settings say.
    assert!(
        !compose.content.contains("stackvo-ui"),
        "the retired containerised UI was emitted"
    );

    // Stack-shaping defaults are no longer written into a fresh `.env`, which
    // only works if they reach the renderer from the binary instead. A
    // workspace whose routes lost their domain, or whose services lost their
    // network, would still render and still start — and be unreachable. That
    // is the failure this pins down.
    //
    // Read from the routing and the base stack rather than from the assembled
    // services, which is where they used to be checked: with nothing switched
    // on that file has no labels to carry a suffix, so the assertion would
    // have gone quiet rather than gone red — the failure mode this whole file
    // exists to avoid.
    let routes = files
        .iter()
        .find(|f| f.path.ends_with("routes.yml"))
        .expect("traefik routes should be rendered");
    assert!(
        routes.content.contains("stackvo.loc"),
        "the embedded domain suffix did not reach the routers"
    );

    let base = files
        .iter()
        .find(|f| f.path.ends_with("stackvo.yml"))
        .expect("the base compose file should be rendered");
    assert!(
        base.content.contains("stackvo-net"),
        "the embedded network name did not reach the base stack"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The project tree somewhere else entirely.
///
/// Every path the generator writes is either the app's or the user's, and until
/// the split they were the same string — so nothing could tell whether a given
/// one had been threaded through correctly. Two directories with nothing in
/// common is the only arrangement in which a mistake shows up: a mount that
/// still points under the app root would find an empty directory and serve a
/// blank page, which is a failure with no error message anywhere.
#[test]
fn the_project_tree_can_live_outside_the_app_directory() {
    let base = std::env::temp_dir().join(format!(
        "stackvo-split-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let app = base.join("app");
    let code = base.join("elsewhere/my-code");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&app).expect("app dir");
    std::fs::create_dir_all(code.join("shop")).expect("php project");
    std::fs::create_dir_all(code.join("api")).expect("node project");

    skeleton::install(&app).expect("install");
    workspace::point_at_projects(&app, &code).expect("pointer");

    std::fs::write(
        code.join("shop/stackvo.json"),
        r#"{"name":"shop","domain":"shop.stackvo.loc","runtime":"php","server":"nginx","php":{"version":"8.4"}}"#,
    )
    .unwrap();
    std::fs::write(
        code.join("api/stackvo.json"),
        r#"{"name":"api","domain":"api.stackvo.loc","runtime":"node","node":{"version":"20","port":3000}}"#,
    )
    .unwrap();

    let (files, skipped) = commands::render_generated(&app).expect("render");
    assert!(skipped.is_empty(), "{skipped:?}");

    let compose = files
        .iter()
        .find(|f| f.path.ends_with("docker-compose.projects.yml"))
        .expect("the projects compose file");
    let text = &compose.content;
    let code_str = code.display().to_string();
    let app_str = app.display().to_string();

    // The source mount follows the user.
    assert!(
        text.contains(&format!("{code_str}/shop:/var/www/html")),
        "the source mount did not follow the project tree:\n{text}"
    );
    // The log mount stays with the app — that output is not the user's.
    assert!(
        text.contains(&format!("{app_str}/logs/projects/shop:")),
        "the log mount left the app directory:\n{text}"
    );
    // A Node image is built from the source, so its context has to be the real
    // directory. `../projects/api` — what this emitted before the split — would
    // resolve under the app root and build an empty context.
    assert!(
        text.contains(&format!("context: {code_str}/api")),
        "the node build context did not follow the project tree:\n{text}"
    );
    assert!(
        !text.contains("context: ../projects/"),
        "a relative node context survived:\n{text}"
    );

    // The Dockerfiles land on the matching side of the line: PHP's is app
    // output, Node's has to sit in the source it is built from.
    let php_dockerfile = files
        .iter()
        .find(|f| f.label == "shop/Dockerfile")
        .expect("php dockerfile");
    assert!(
        php_dockerfile.path.starts_with(&app),
        "{:?}",
        php_dockerfile.path
    );
    let node_dockerfile = files
        .iter()
        .find(|f| f.label == "api/Dockerfile")
        .expect("node dockerfile");
    assert!(
        node_dockerfile.path.starts_with(&code),
        "{:?}",
        node_dockerfile.path
    );

    std::fs::remove_dir_all(&base).ok();
}
