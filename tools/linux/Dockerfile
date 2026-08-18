# The Linux half of this repository, runnable without waiting for CI.
#
# §3 #12 and #35 both said the same thing: the code is written, the job is
# wired, and nobody has ever seen it run. On macOS `tauri-driver` does not
# exist and `elevate.rs`'s polkit branch compiles out, so "it will be fine"
# was the only available answer — which is the answer this repository refuses
# everywhere else.
#
# Deliberately the same base and the same packages as `.github/workflows/ci.yml`
# installs, so a green run here means something about the runner rather than
# about this file. `contract_spelling.rs` and its neighbours have the habit:
# when two places state one fact, the second one is the one that goes stale.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_TERM_COLOR=always

# The line below is kept identical to ci.yml's, plus what a container has to
# add for itself: a runner image already has curl, git, build-essential and a
# Rust toolchain.
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev libappindicator3-dev \
      librsvg2-dev patchelf libssl-dev libdbus-1-dev \
      build-essential curl git ca-certificates pkg-config \
      xvfb webkit2gtk-driver \
    && rm -rf /var/lib/apt/lists/*

# Pinned from `src-tauri/rust-toolchain.toml`, which rustup reads from the
# working directory — installing "stable" here would put components on a
# toolchain the build never uses. That exact mistake is already commented in
# ci.yml's coverage job.
ARG RUST_VERSION=1.96.1
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain "${RUST_VERSION}" --profile minimal \
      --component rustfmt --component clippy
ENV PATH="/root/.cargo/bin:${PATH}"

# Node, because the driver suite (§3 #12) builds the front end and runs the
# tests with it. CI uses `actions/setup-node@v4` with 22; matching the major is
# what makes a green run here mean anything about a green run there.
ARG NODE_MAJOR=22
RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /repo
