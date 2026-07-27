#!/usr/bin/env bash
#
# Regenerate the differential-test fixtures from the real Bash generator.
#
# Runs the generator in a throwaway sandbox — a copy of the checkout's `core/`
# and `.env` plus synthetic probe projects — so the user's own projects and
# generated output are never touched. Read-only against the source checkout.
#
# Run this when the Bash generator changes; the resulting git diff shows exactly
# what changed about the images StackVo produces.
#
#   tools/make-fixtures.sh [path-to-stackvo]

set -euo pipefail

ROOT="${1:-${STACKVO_ROOT:-$(cd "$(dirname "$0")/../../stackvo" && pwd)}}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$HERE/src-tauri/tests/fixtures"

if [ ! -f "$ROOT/core/cli/stackvo.sh" ]; then
  echo "Not a StackVo checkout: $ROOT" >&2
  exit 1
fi

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

cp -R "$ROOT/core" "$SANDBOX/core"
cp "$ROOT/.env" "$SANDBOX/.env"
mkdir -p "$SANDBOX/projects" "$SANDBOX/generated"

# One PHP probe per web server, all with the same extension list so the only
# variable between fixtures is the server template.
for srv in nginx apache caddy frankenphp swoole; do
  mkdir -p "$SANDBOX/projects/probe-$srv/public"
  cat > "$SANDBOX/projects/probe-$srv/stackvo.json" <<JSON
{
  "name": "probe-$srv",
  "domain": "probe-$srv.loc",
  "runtime": "php",
  "server": "$srv",
  "document_root": "public",
  "php": {
    "version": "8.4",
    "extensions": [
      "mbstring",
      "pdo",
      "pdo_mysql",
      "curl",
      "zip",
      "gd",
      "redis",
      "xdebug",
      "intl",
      "bcmath"
    ]
  }
}
JSON
done

mkdir -p "$SANDBOX/projects/probe-node"
cat > "$SANDBOX/projects/probe-node/stackvo.json" <<'JSON'
{
  "name": "probe-node",
  "domain": "probe-node.loc",
  "runtime": "node",
  "node": {
    "version": "22",
    "install": "npm install",
    "build": "npm run build",
    "start": "node .output/server/index.mjs",
    "port": 3000
  }
}
JSON

( cd "$SANDBOX" && bash core/cli/stackvo.sh generate projects >/dev/null 2>&1 )

rm -rf "$FIXTURES"
mkdir -p "$FIXTURES"

for srv in nginx apache caddy frankenphp swoole; do
  mkdir -p "$FIXTURES/probe-$srv"
  cp "$SANDBOX/projects/probe-$srv/stackvo.json" "$FIXTURES/probe-$srv/"
  cp "$SANDBOX/generated/projects/probe-$srv/Dockerfile" "$FIXTURES/probe-$srv/"
done

# Node writes its Dockerfile into the project SOURCE directory, not
# generated/projects — the build context has to be the real source for
# `COPY . .` to work. See CONFLICTS.md C-19.
mkdir -p "$FIXTURES/probe-node"
cp "$SANDBOX/projects/probe-node/stackvo.json" "$FIXTURES/probe-node/"
cp "$SANDBOX/projects/probe-node/Dockerfile" "$FIXTURES/probe-node/"
cp "$SANDBOX/projects/probe-node/.dockerignore" "$FIXTURES/probe-node/"

# The compose file embeds absolute host paths in its bind mounts — the one
# machine-specific part of the generated output. Replace the sandbox path with
# a placeholder so the fixture is reproducible; the test renders with the same
# placeholder as its host root.
sed "s|$SANDBOX|__ROOT__|g" "$SANDBOX/generated/docker-compose.projects.yml" \
  > "$FIXTURES/docker-compose.projects.yml"

# Traefik: two variants, because SSL_ENABLE changes both files and the off
# variant is what exposes C-20.
mkdir -p "$FIXTURES/traefik"
for ssl in true false; do
  ( cd "$SANDBOX" \
    && sed -i.bak "s/^SSL_ENABLE=.*/SSL_ENABLE=$ssl/" .env \
    && sed -i.bak "s/^SERVICE_RABBITMQ_ENABLE=.*/SERVICE_RABBITMQ_ENABLE=true/" .env \
    && sed -i.bak "s/^SERVICE_MAILHOG_ENABLE=.*/SERVICE_MAILHOG_ENABLE=true/" .env \
    && sed -i.bak "s/^SERVICE_KIBANA_ENABLE=.*/SERVICE_KIBANA_ENABLE=$ssl/" .env \
    && sed -i.bak "s/^SERVICE_GRAFANA_ENABLE=.*/SERVICE_GRAFANA_ENABLE=$ssl/" .env \
    && bash core/cli/stackvo.sh generate >/dev/null 2>&1 )

  suffix=$([ "$ssl" = "true" ] && echo ssl || echo nossl)
  cp "$SANDBOX/generated/traefik/dynamic/routes.yml" "$FIXTURES/traefik/routes-$suffix.yml"
  cp "$SANDBOX/generated/traefik/traefik.yml" "$FIXTURES/traefik/traefik-$suffix.yml"
done

# Freeze the toolchain values the fixtures were built with, so a later change
# to the user's .env cannot silently invalidate the comparison.
grep -E '^PHP_DEFAULT_TOOLS|^PHP_DEFAULT_APT_PACKAGES|^PHP_TOOL_' "$SANDBOX/.env" \
  > "$FIXTURES/toolchain.env"

echo "Fixtures regenerated from $ROOT"
find "$FIXTURES" -type f | sed "s|$FIXTURES/|  |" | sort
