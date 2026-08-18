#!/usr/bin/env bash
# Run the Linux-only halves of the suite, on this machine, in a container.
#
#   tools/linux/run.sh                      # the probes that only exist on Linux
#   tools/linux/run.sh --test elevate_probe # one of them
#   tools/linux/run.sh --driver             # the tauri-driver suite (#12), with a display
#   tools/linux/run.sh --shell              # a prompt inside the image
#
# The image caches; the cargo registry and target directory are bind-mounted to
# named volumes so a second run is a rebuild rather than a fresh one.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
image=stackvo-linux

if ! docker image inspect "$image" >/dev/null 2>&1; then
  echo "building $image (once) ..."
  docker build -t "$image" "$repo/tools/linux"
fi

docker volume create stackvo-cargo >/dev/null
docker volume create stackvo-target >/dev/null
# node_modules gets its OWN volume, mounted over the bind. Without it the
# container's `npm ci` writes Linux binaries into the host checkout and the next
# `npx vitest` on the Mac dies on a missing `@rollup/rollup-darwin-arm64` — a
# container that breaks the machine it was run from is not a convenience.
docker volume create stackvo-node-modules >/dev/null

# `--jobs 1` for the link step, not for speed. Docker Desktop's VM defaults to
# well under what this crate's linker wants — GTK, WebKit, bollard, rustls and
# aws-lc in one binary — and parallel `ld` invocations get OOM-killed with
# `signal 9`, which reads as a linker bug rather than as memory. Compiling is
# not the expensive part; linking three test binaries at once is.
run() {
  docker run --rm -i \
    -v "$repo:/repo" \
    -v stackvo-cargo:/root/.cargo/registry \
    -v stackvo-target:/repo/src-tauri/target-linux \
    -v stackvo-node-modules:/repo/node_modules \
    -e CARGO_TARGET_DIR=/repo/src-tauri/target-linux \
    -e CARGO_BUILD_JOBS="${STACKVO_LINUX_JOBS:-1}" \
    -w /repo/src-tauri \
    "$image" "$@"
}

if [ "${1:-}" = "--shell" ]; then
  exec docker run --rm -it \
    -v "$repo:/repo" \
    -v stackvo-cargo:/root/.cargo/registry \
    -v stackvo-target:/repo/src-tauri/target-linux \
    -v stackvo-node-modules:/repo/node_modules \
    -e CARGO_TARGET_DIR=/repo/src-tauri/target-linux \
    -w /repo "$image" bash
fi

# §3 #12. The driver suite needs more than a plain `cargo test`: the front end
# built, the application built the way it SHIPS, `tauri-driver` on PATH, and a
# display, because the app opens a window.
#
# `npx tauri build --debug --no-bundle`, not `cargo build`. A plain cargo build
# embeds `devUrl` — the webview opens http://localhost:1420, gets "connection
# refused", and every test reports an empty `#app` with no explanation. So the
# headline test of this suite, "the built bundle renders inside the real
# webview", could never pass with the profile the suite itself chose.
# `--no-bundle` because what is wanted is a binary to drive, not a .deb.
#
# `STACKVO_DRIVER_BINARY` because the container builds into `target-linux/` (so
# a Linux object file never lands in the macOS target dir and invalidates it),
# while `binaryPath()` looks in `target/`.
if [ "${1:-}" = "--driver" ]; then
  run bash -lc '
    set -euo pipefail
    cd /repo
    npm ci --no-audit --no-fund
    npm run build
    npx tauri build --debug --no-bundle
    command -v tauri-driver >/dev/null || cargo install tauri-driver --locked
    STACKVO_DRIVER_BINARY=/repo/src-tauri/target-linux/debug/stackvo-desktop \
      xvfb-run -a --server-args="-screen 0 1280x820x24" npm run test:driver
  '
  exit $?
fi

# The exit status is the point of running this at all: the first version of
# this script ended on a pipeline whose status was the tail's, so a failing
# compile reported success. A runner that cannot fail is not a runner.
if [ $# -gt 0 ]; then
  run cargo test "$@"
  exit $?
else
  # The two that cannot run on the developer's machine at all. Named rather
  # than "everything": a full run here would be a slower copy of what `cargo
  # test` already answers on macOS, and this is for the answers it cannot give.
  run cargo test --test elevate_probe --test hosts_roundtrip
  exit $?
fi
