//! Reading a rival's installation off a real disk, and copying a site out of it.
//!
//! The pure tests in `imports.rs` settle what a site is and what a vhost says.
//! What only exists once there are directories is the rest of it: that an
//! XAMPP tree is recognised by its `htdocs` and a Laragon one by its `www`,
//! that the tool's own dashboard is not offered as a project, that a Laragon
//! vhost reaches the site it belongs to, and — the one that matters — that a
//! copy is a copy and the other installation is left exactly as it was.

use stackvo_desktop_lib::imports::{self, Source};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("stackvo-foreign-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A XAMPP installation with two real sites and the dashboard it ships.
fn xampp(root: &std::path::Path) {
    let htdocs = root.join("htdocs");

    // A Laravel application: `artisan` is a marker only Laravel has.
    write(&htdocs.join("shop/artisan"), "#!/usr/bin/env php\n");
    write(
        &htdocs.join("shop/composer.json"),
        r#"{"require":{"php":"^8.2","laravel/framework":"^11.0"}}"#,
    );
    write(&htdocs.join("shop/public/index.php"), "<?php\n");

    // Plain PHP, served from the root.
    write(&htdocs.join("legacy/index.php"), "<?php\n");

    // XAMPP's own, which must not be offered.
    write(&htdocs.join("dashboard/index.html"), "<html>\n");
    write(&htdocs.join("webalizer/index.html"), "<html>\n");
    write(&htdocs.join("applications.html"), "<html>\n");
}

#[test]
fn an_xampp_tree_yields_its_sites_and_not_its_dashboard() {
    let root = scratch("xampp");
    xampp(&root);

    let install = imports::scan_at(Source::Xampp, &root, None).expect("an XAMPP installation");
    let names: Vec<&str> = install.sites.iter().map(|s| s.name.as_str()).collect();

    assert_eq!(
        names,
        ["legacy", "shop"],
        "sorted, and the tool's own excluded"
    );

    let shop = install.sites.iter().find(|s| s.name == "shop").unwrap();
    // The same inference an ordinary adoption makes — an imported project is
    // not a second class of project.
    assert_eq!(shop.detected.framework, Some("laravel"));
    assert_eq!(shop.detected.document_root.as_deref(), Some("public"));
    assert_eq!(shop.detected.php_version.as_deref(), Some("8.2"));
    // XAMPP serves by path and has no hostname to read, so adoption asks.
    assert!(shop.domain.is_none());
    assert!(shop.bytes > 0 && !shop.partial);

    // Legacy PHP is served from the directory itself, not from `public/` —
    // getting this wrong builds, starts and serves a 404 with no error.
    let legacy = install.sites.iter().find(|s| s.name == "legacy").unwrap();
    assert_eq!(legacy.detected.document_root.as_deref(), Some("."));

    // A directory that is not an installation is not one.
    assert!(imports::scan_at(Source::Laragon, &root, None).is_none());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_laragon_site_carries_the_hostname_its_vhost_declares() {
    let root = scratch("laragon");

    write(&root.join("www/blog/index.php"), "<?php\n");
    write(&root.join("www/api/index.php"), "<?php\n");
    write(
        &root.join("etc/apache2/sites-enabled/auto.blog.test.conf"),
        "<VirtualHost *:80>\n  DocumentRoot \"C:/laragon/www/blog\"\n  ServerName blog.test\n</VirtualHost>\n",
    );

    let install = imports::scan_at(Source::Laragon, &root, None).expect("a Laragon installation");

    let blog = install.sites.iter().find(|s| s.name == "blog").unwrap();
    assert_eq!(blog.domain.as_deref(), Some("blog.test"));

    // A site with no vhost still imports; it simply has no name to carry, and
    // adoption falls back to the suffix as it does for every other project.
    let api = install.sites.iter().find(|s| s.name == "api").unwrap();
    assert!(api.domain.is_none());

    let _ = std::fs::remove_dir_all(&root);
}

/// A name already under `projects/` is reported before anything is clicked.
#[test]
fn a_name_that_is_taken_is_said_so_in_the_listing() {
    let root = scratch("taken");
    xampp(&root);

    let projects = root.join("workspace-projects");
    std::fs::create_dir_all(projects.join("shop")).unwrap();

    let install = imports::scan_at(Source::Xampp, &root, Some(&projects)).unwrap();
    assert!(
        install
            .sites
            .iter()
            .find(|s| s.name == "shop")
            .unwrap()
            .taken
    );
    assert!(
        !install
            .sites
            .iter()
            .find(|s| s.name == "legacy")
            .unwrap()
            .taken
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The promise the module opens with: nothing is written to the other
/// installation, and a copy is a copy.
#[test]
fn copying_a_site_leaves_the_original_untouched() {
    let root = scratch("copy");
    xampp(&root);

    let source = root.join("htdocs/shop");
    let target = root.join("projects/shop");

    imports::copy_tree(&source, &target).expect("copying the tree");

    for relative in ["artisan", "composer.json", "public/index.php"] {
        assert!(
            target.join(relative).is_file(),
            "{relative} did not come over"
        );
        assert!(
            source.join(relative).is_file(),
            "{relative} left the source"
        );
    }

    // Byte for byte, not "a file of that name exists".
    assert_eq!(
        std::fs::read(source.join("composer.json")).unwrap(),
        std::fs::read(target.join("composer.json")).unwrap()
    );

    // And the copy is a project the ordinary detection recognises, which is
    // what makes the adoption that follows the same adoption as any other.
    assert_eq!(
        stackvo_desktop_lib::detect::detect(&target).framework,
        Some("laravel")
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A symlink is not followed. One pointing at `/` turns a copy of a site into a
/// copy of the disk, and one pointing back into the tree is a loop.
#[test]
fn a_symlink_is_not_walked_into() {
    let root = scratch("symlink");
    write(&root.join("htdocs/site/index.php"), "<?php\n");
    std::fs::create_dir_all(root.join("elsewhere/secret")).unwrap();
    write(&root.join("elsewhere/secret/keys.txt"), "hunter2\n");

    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join("htdocs/site/link")).unwrap();

    let target = root.join("projects/site");
    imports::copy_tree(&root.join("htdocs/site"), &target).expect("copying");

    assert!(target.join("index.php").is_file());
    assert!(
        !target.join("link").exists(),
        "a symlink was followed out of the site being copied"
    );

    let _ = std::fs::remove_dir_all(&root);
}
