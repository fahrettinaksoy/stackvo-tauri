//! Would an import find the sites? (L)
//!
//! `imports.rs` reads five other tools' installations, and four of the five
//! cannot be exercised on the machine this was written on: XAMPP, Laragon and
//! MAMP are not installed, and Valet is a composer package that is not here
//! either. Its unit tests therefore drive the pure functions with strings.
//!
//! What no string can check is the part that has been wrong twice in this
//! session already: whether the **directory layout** each tool actually
//! produces is the layout the scanner walks. A parser that reads a fixture its
//! own author wrote is a parser that agrees with its author.
//!
//! So this builds each tool's real layout on disk — from the shapes their own
//! documentation and installers produce — and runs the shipped scanner over it:
//!
//! ```sh
//! cargo run --example import_probe
//! ```
//!
//! Everything is created under the OS temp directory and removed on the way
//! out. Nothing on this machine is read, moved or written.

use stackvo_desktop_lib::imports::{scan_at, Source};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let root = std::env::temp_dir().join(format!("stackvo-import-probe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    if fs::create_dir_all(&root).is_err() {
        println!("could not make a scratch directory");
        return;
    }

    let mut failures = 0;
    for case in cases(&root) {
        let found = scan_at(case.source, &case.at, None);
        let sites: Vec<String> = found
            .as_ref()
            .map(|install| install.sites.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();

        let domains: Vec<String> = found
            .as_ref()
            .map(|install| {
                install
                    .sites
                    .iter()
                    .filter_map(|s| s.domain.clone())
                    .collect()
            })
            .unwrap_or_default();
        let services: Vec<String> = found
            .as_ref()
            .map(|install| {
                install
                    .sites
                    .iter()
                    .flat_map(|s| s.services.clone())
                    .collect()
            })
            .unwrap_or_default();

        let ok = sites == case.expect_sites
            && domains == case.expect_domains
            && services == case.expect_services;
        if !ok {
            failures += 1;
        }

        let (shown_sites, shown_domains) = (format!("{sites:?}"), format!("{domains:?}"));
        println!(
            "  {} {:<9} sites={shown_sites:<28} domains={shown_domains:<18} services={services:?}",
            if ok { "ok  " } else { "FAIL" },
            case.source.as_str(),
        );
        if !ok {
            println!(
                "       expected sites={:?} domains={:?} services={:?}",
                case.expect_sites, case.expect_domains, case.expect_services
            );
        }
    }

    let _ = fs::remove_dir_all(&root);
    println!();
    if failures == 0 {
        println!("every tool's own layout was read as the sites it holds.");
    } else {
        println!("{failures} layout(s) were not read correctly.");
    }
}

struct Case {
    source: Source,
    at: PathBuf,
    expect_sites: Vec<String>,
    expect_domains: Vec<String>,
    expect_services: Vec<String>,
}

fn site(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("index.php"), "<?php echo 1;").unwrap();
    path
}

fn cases(root: &Path) -> Vec<Case> {
    let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // ---- XAMPP: htdocs, with the dashboard it ships ------------------------
    let xampp = root.join("xamppfiles");
    let htdocs = xampp.join("htdocs");
    fs::create_dir_all(&htdocs).unwrap();
    site(&htdocs, "shop");
    site(&htdocs, "blog");
    site(&htdocs, "dashboard");
    fs::write(htdocs.join("index.php"), "<?php").unwrap();

    // ---- MAMP: the same shape, its own prefix ------------------------------
    let mamp = root.join("MAMP");
    let mamp_htdocs = mamp.join("htdocs");
    fs::create_dir_all(&mamp_htdocs).unwrap();
    site(&mamp_htdocs, "invoices");

    // ---- Laragon: www, plus a generated vhost per site ---------------------
    let laragon = root.join("laragon");
    let www = laragon.join("www");
    fs::create_dir_all(&www).unwrap();
    site(&www, "crm");
    let vhosts = laragon.join("etc/apache2/sites-enabled");
    fs::create_dir_all(&vhosts).unwrap();
    fs::write(
        vhosts.join("auto.crm.test.conf"),
        "<VirtualHost *:80>\n    DocumentRoot \"C:/laragon/www/crm\"\n    ServerName crm.test\n    ServerAlias *.crm.test\n</VirtualHost>\n",
    )
    .unwrap();

    // ---- Valet: a parked directory and a linked one ------------------------
    //
    // Valet's config names parked paths in `config.json`; links are symlinks
    // under `Sites/`. Both are real ways a Valet site exists and the scanner
    // has to read both.
    let valet = root.join("valet-config");
    fs::create_dir_all(valet.join("Sites")).unwrap();
    let parked = root.join("Code");
    fs::create_dir_all(&parked).unwrap();
    site(&parked, "api");
    // Linked only where symlinks exist; on Windows the probe covers the parked
    // directory and says nothing about a link it could not make.
    #[cfg_attr(not(unix), allow(unused_variables))]
    let linked = site(&root.join("elsewhere"), "legacy");
    fs::write(
        valet.join("config.json"),
        format!(
            "{{\n  \"tld\": \"test\",\n  \"paths\": [\"{}\"],\n  \"loopback\": \"127.0.0.1\"\n}}\n",
            parked.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&linked, valet.join("Sites").join("legacy")).unwrap();

    // ---- Sail: a project, and a folder holding one -------------------------
    let code = root.join("sail-code");
    let shop = code.join("shop");
    fs::create_dir_all(&shop).unwrap();
    fs::write(shop.join("artisan"), "#!/usr/bin/env php\n").unwrap();
    fs::write(
        shop.join("composer.json"),
        "{\"require\":{\"laravel/framework\":\"^11\"}}",
    )
    .unwrap();
    fs::write(
        shop.join(".env"),
        "APP_NAME=Shop\nAPP_URL=http://shop.test\n",
    )
    .unwrap();
    // Four-space indentation, which is what `sail:install` writes — and what a
    // two-space rule found nothing in.
    fs::write(
        shop.join("docker-compose.yml"),
        r#"services:
    laravel.test:
        build:
            context: './vendor/laravel/sail/runtimes/8.4'
        image: 'sail-8.4/app'
    pgsql:
        image: 'postgres:16'
    redis:
        image: 'redis:alpine'
    meilisearch:
        image: 'getmeili/meilisearch:latest'
volumes:
    sail-pgsql:
        driver: local
"#,
    )
    .unwrap();

    vec![
        Case {
            source: Source::Xampp,
            at: xampp,
            // `dashboard` is XAMPP's own and is not a site.
            expect_sites: names(&["blog", "shop"]),
            expect_domains: vec![],
            expect_services: vec![],
        },
        Case {
            source: Source::Mamp,
            at: mamp,
            expect_sites: names(&["invoices"]),
            expect_domains: vec![],
            expect_services: vec![],
        },
        Case {
            source: Source::Laragon,
            at: laragon,
            expect_sites: names(&["crm"]),
            // Read out of the generated vhost — ServerName, never ServerAlias.
            expect_domains: names(&["crm.test"]),
            expect_services: vec![],
        },
        Case {
            source: Source::Valet,
            at: valet,
            // Parked and linked, both, with the tld from Valet's own config.
            expect_sites: names(&["api", "legacy"]),
            expect_domains: names(&["api.test", "legacy.test"]),
            expect_services: vec![],
        },
        Case {
            source: Source::Sail,
            at: code,
            expect_sites: names(&["shop"]),
            expect_domains: names(&["shop.test"]),
            // `laravel.test` is the application; meilisearch has no counterpart.
            expect_services: names(&["postgres", "redis"]),
        },
    ]
}
