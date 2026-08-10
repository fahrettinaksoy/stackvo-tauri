//! The generator's hot path, measured.
//!
//! `template::render` is the function every generated file goes through: 32 of
//! them on a real checkout, each rendered line by line with two substitution
//! passes. It is the only piece of this codebase that does real per-byte work
//! on a path a user waits on, so if anything here is worth a number, it is this.
//!
//! ## This is an instrument, not a gate — and that is a decision, not an omission
//!
//! The obvious next step is to run this in CI and fail the build on a
//! regression. That would be a mistake, and it is worth writing down why rather
//! than leaving the next person to rediscover it.
//!
//! GitHub's runners are shared machines with variable CPU and noisy
//! neighbours. Run-to-run variance on a hosted runner is routinely wider than
//! the regressions anybody wants to catch, so a threshold tight enough to be
//! useful fails on runs where nothing changed, and one loose enough to be
//! stable catches nothing. Either way the outcome is the same, and this
//! repository has already written it down once: a red gate that people learn to
//! ignore is worse than no gate — §36.1 of the readiness report makes exactly
//! this argument about flooring a coverage number nobody can act on.
//!
//! So the bundle budget next door **is** a gate, because bytes are the same on
//! every machine, and this one is not, because nanoseconds are not. Run it on
//! purpose, on a quiet machine, when a change is meant to affect it:
//!
//! ```text
//! cargo bench --manifest-path src-tauri/Cargo.toml
//! ```
//!
//! Criterion keeps its own history in `target/criterion`, so a second run on
//! the same machine reports the delta — which is the comparison that means
//! something, unlike a comparison against a number recorded on a different
//! machine a month ago.

use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::BTreeMap;
use std::hint::black_box;

use stackvo_desktop_lib::template;

/// A substitution table the size of a real `.env`.
///
/// Sized from the shipped example rather than picked: a table of three entries
/// measures hashing that does not happen in production, because both
/// substitution passes look every candidate up in this map.
fn env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for (key, value) in [
        ("STACKVO_ROOT", "/Users/dev/stackvo"),
        ("HOST_STACKVO_ROOT", "/Users/dev/stackvo"),
        ("HOST_UID", "501"),
        ("HOST_GID", "20"),
        ("DEFAULT_TLD_SUFFIX", "loc"),
        ("DEFAULT_PHP_VERSION", "8.3"),
        ("DEFAULT_NODE_VERSION", "22"),
        ("DEFAULT_WEBSERVER", "nginx"),
        ("SSL_ENABLE", "true"),
        ("MYSQL_ROOT_PASSWORD", "secret"),
        ("MYSQL_DATABASE", "stackvo"),
        ("REDIS_PORT", "6379"),
        ("POSTGRES_PORT", "5432"),
        ("TRAEFIK_DASHBOARD_PORT", "8080"),
        ("MAILPIT_HTTP_PORT", "8025"),
    ] {
        env.insert(key.to_string(), value.to_string());
    }
    env
}

/// A compose service block, which is the shape most templates actually are.
fn service_template() -> String {
    let block = r#"
  redis:
    image: redis:7-alpine
    container_name: stackvo-redis
    restart: unless-stopped
    ports:
      - "${REDIS_PORT}:6379"
    volumes:
      - ${HOST_STACKVO_ROOT}/generated/configs/redis.conf:/usr/local/etc/redis/redis.conf
    networks:
      - stackvo-net
    environment:
      TZ: "${TZ}"
      ROOT: "$STACKVO_ROOT"
"#;
    // Twenty of them: `docker-compose.dynamic.yml` is assembled from the whole
    // service catalog, so a single block understates the real input.
    block.repeat(20)
}

fn benchmarks(c: &mut Criterion) {
    let env = env();
    let template = service_template();

    c.bench_function("render a compose file", |b| {
        b.iter(|| template::render(black_box(&template), black_box(&env)))
    });

    // The no-substitution case, for contrast. Most lines of most templates
    // contain no variable at all, and the cost of walking them is what a
    // "faster substitution" change would have to beat.
    let plain = template.replace('$', "#");
    c.bench_function("render a file with no variables", |b| {
        b.iter(|| template::render(black_box(&plain), black_box(&env)))
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
