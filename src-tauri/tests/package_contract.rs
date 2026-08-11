//! `pkg::Manifest` and `contracts/package-version.schema.json` describe the
//! same object.
//!
//! Two files written by hand, in two languages, about one document. The drift
//! this catches is the quiet kind: a field added to the schema and not to the
//! struct is a field `serde` throws away — packages carry it, the client reads
//! past it, and nothing anywhere says so. The reverse is worse only because it
//! is louder: a field the struct has and the schema forbids makes every
//! manifest that uses it invalid, which is how `recommendedVersion` was a
//! violation of `package.schema.json` in twenty-five files at once until this
//! kind of check existed.
//!
//! It is the same argument `contract_agreement.rs` makes for `ipc.json` — the
//! contract is written rather than generated (ADR 0006), and what makes that
//! affordable is a test that fails when the two halves disagree.

use std::collections::BTreeSet;
use std::path::PathBuf;

use stackvo_desktop_lib::pkg;

fn contracts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts")
}

fn schema(name: &str) -> serde_json::Value {
    let path = contracts().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn property_names(schema: &serde_json::Value) -> BTreeSet<String> {
    schema["properties"]
        .as_object()
        .expect("the schema declares properties")
        .keys()
        .cloned()
        .collect()
}

/// Every field, populated, so serialisation cannot omit one.
///
/// Built through the parser rather than as a struct literal: a literal would
/// still compile if `serde` renamed a field, and the rename is exactly what
/// this is looking for.
fn full_manifest() -> pkg::Manifest {
    let sha = "a".repeat(64);
    let text = format!(
        r#"{{
          "apiVersion": "{}",
          "service": "mysql",
          "version": "8.0",
          "image": {{"registry": "docker.io", "repository": "mysql", "tag": "8.0",
                     "digest": "sha256:{sha}"}},
          "capabilities": ["sql"],
          "instancing": {{"multiple": true, "identity": "version"}},
          "ports": [{{"name": "main", "container": 3306, "preferred": 3306,
                      "protocol": "tcp", "legacyKey": "HOST_PORT_MYSQL", "primary": true}}],
          "volumes": [{{"name": "data", "container": "/var/lib/mysql", "purgeable": true}}],
          "files": [{{"name": "my_cnf", "template": "files/my.cnf.tpl",
                      "target": "/etc/mysql/conf.d/stackvo.cnf", "mode": "0444",
                      "sha256": "{sha}"}}],
          "settings": [{{"key": "ROOT_PASSWORD", "type": "secret", "default": "root",
                         "required": true, "options": [], "capability": null,
                         "label": {{"en": "Root password"}}}}],
          "connection": {{"scheme": "mysql", "port": "main", "userSetting": null,
                          "defaultUser": "root", "passwordSetting": "ROOT_PASSWORD",
                          "databaseSetting": null, "defaultDatabase": "stackvo",
                          "options": {{"authSource": "admin"}}}},
          "url": {{"subdomain": "phpmyadmin", "port": "main"}},
          "health": {{"test": ["CMD", "mysqladmin", "ping"], "interval": "10s",
                      "timeout": "5s", "retries": 12, "startPeriod": "30s"}},
          "dependsOn": [{{"capability": "sql", "service": "mysql", "required": false}}],
          "companions": [{{"name": "zookeeper",
                           "image": {{"repository": "confluentinc/cp-zookeeper", "tag": "latest"}},
                           "ports": [], "volumes": [],
                           "compose": {{"file": "companion.zookeeper.yml.tpl", "sha256": "{sha}"}}}}],
          "compose": {{"file": "compose.yml.tpl", "sha256": "{sha}"}},
          "support": {{"status": "supported", "eolDate": "2026-10-25",
                       "source": "https://endoflife.date/api/mysql.json"}},
          "notes": {{"en": "note"}}
        }}"#,
        pkg::API_VERSION
    );
    pkg::parse(&text).expect("the example in this test is a manifest the client accepts")
}

/// The struct's fields and the schema's properties are the same set.
#[test]
fn the_manifest_struct_and_its_schema_carry_the_same_fields() {
    let schema = schema("package-version.schema.json");
    let declared = property_names(&schema);

    let json = serde_json::to_value(full_manifest()).expect("serialising the manifest");
    let serialised: BTreeSet<String> = json
        .as_object()
        .expect("a manifest serialises to an object")
        .keys()
        .cloned()
        .collect();

    let missing_from_rust: Vec<&String> = declared.difference(&serialised).collect();
    assert!(
        missing_from_rust.is_empty(),
        "the schema declares {missing_from_rust:?} and `pkg::Manifest` has no field for them — \
         serde discards those silently, so a package could carry one and the client would \
         never read it"
    );

    let missing_from_schema: Vec<&String> = serialised.difference(&declared).collect();
    assert!(
        missing_from_schema.is_empty(),
        "`pkg::Manifest` serialises {missing_from_schema:?} and the schema does not declare \
         them — with additionalProperties:false that makes every manifest carrying one invalid"
    );
}

/// Nested objects agree too. These are the ones a top-level comparison misses.
#[test]
fn the_nested_shapes_agree_field_for_field() {
    let schema = schema("package-version.schema.json");
    let json = serde_json::to_value(full_manifest()).unwrap();

    // (schema pointer to the object's node, pointer into the serialised value)
    let pairs: [(&str, &str); 9] = [
        ("/properties/image", "/image"),
        ("/properties/instancing", "/instancing"),
        ("/properties/ports/items", "/ports/0"),
        ("/properties/volumes/items", "/volumes/0"),
        ("/properties/files/items", "/files/0"),
        ("/properties/settings/items", "/settings/0"),
        ("/properties/connection", "/connection"),
        ("/properties/support", "/support"),
        ("/properties/health", "/health"),
    ];

    for (schema_at, value_at) in pairs {
        let node = schema
            .pointer(schema_at)
            .unwrap_or_else(|| panic!("the schema has no node at {schema_at}"));
        let declared: BTreeSet<String> = node["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{schema_at} declares no properties"))
            .keys()
            .cloned()
            .collect();

        let value = json
            .pointer(value_at)
            .unwrap_or_else(|| panic!("the example manifest has nothing at {value_at}"));
        let serialised: BTreeSet<String> = value
            .as_object()
            .unwrap_or_else(|| panic!("{value_at} is not an object"))
            .keys()
            .cloned()
            .collect();

        assert_eq!(
            declared, serialised,
            "{schema_at} and {value_at} describe different fields"
        );
    }
}

/// A `required` name the schema does not declare is a rule nothing can satisfy.
///
/// Checked here as well as in `validate-contracts.mjs` because these two run at
/// different times: the Node suite is a contributor's check and this one runs in
/// the build that ships.
#[test]
fn every_required_field_is_one_the_schema_declares() {
    for name in [
        "package.schema.json",
        "package-version.schema.json",
        "registry.schema.json",
    ] {
        let schema = schema(name);
        let declared = property_names(&schema);
        for required in schema["required"].as_array().into_iter().flatten() {
            let key = required.as_str().expect("required names are strings");
            assert!(
                declared.contains(key),
                "{name} requires {key:?} and does not declare it"
            );
        }
    }
}

/// The client's refusals are the schema's rules, spelled the same way.
///
/// `MOVING_TAGS` and the schema's `not.enum` are the same list in two files,
/// and they are the list ADR 0014 turns on. A tag that is moving in one and not
/// the other is a version somebody can publish and nobody can install.
#[test]
fn the_moving_tags_are_the_same_list_on_both_sides() {
    let schema = schema("package-version.schema.json");
    let forbidden: BTreeSet<String> = schema["properties"]["version"]["not"]["enum"]
        .as_array()
        .expect("the version property forbids the moving tags")
        .iter()
        .map(|v| v.as_str().expect("tags are strings").to_string())
        .collect();

    let known: BTreeSet<String> = pkg::MOVING_TAGS.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        forbidden, known,
        "package-version.schema.json and pkg::MOVING_TAGS disagree about what a moving tag is"
    );
}

/// Both schemas offer the same categories, and they are the ones the app's own
/// service catalog groups by.
#[test]
fn the_categories_are_one_list_in_three_places() {
    let from = |name: &str, pointer: &str| -> BTreeSet<String> {
        schema(name)
            .pointer(pointer)
            .unwrap_or_else(|| panic!("{name} has no node at {pointer}"))
            .as_array()
            .expect("an enum is an array")
            .iter()
            .map(|v| v.as_str().expect("categories are strings").to_string())
            .collect()
    };

    let package = from("package.schema.json", "/properties/category/enum");
    let registry = from(
        "registry.schema.json",
        "/properties/packages/items/properties/category/enum",
    );
    assert_eq!(package, registry);

    // And against `env.schema.json`, whose groups these were lifted from. The
    // one deliberate difference is spelling: a directory name should not be
    // camelCase, so `adminUis` became `admin-uis`.
    let catalog: BTreeSet<String> = stackvo_desktop_lib::contracts::env_schema()
        .service_catalog()
        .into_iter()
        .map(|(_, category)| kebab(&category))
        .collect();

    assert_eq!(
        package, catalog,
        "the package categories and env.schema.json's service groups have drifted — \
         a package directory named for a category the catalog does not have is a \
         package the migration cannot place"
    );
}

fn kebab(text: &str) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
