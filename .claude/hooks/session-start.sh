#!/bin/bash
# SessionStart hook: prepare a Claude Code on the web session to build and
# verify the WASM web demo (web/index.html) in addition to running cargo tests.
#
# Rust stable is pre-installed in the web environment; the gap is the WASM
# toolchain used by build_web.sh (the wasm32 target + wasm-pack). Everything
# here is idempotent and fast on the common path (no-ops once the container
# state is cached after the first run).
set -euo pipefail

# Web (remote) sessions only; do nothing on local machines.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Run asynchronously: the session starts immediately while the toolchain installs
# in the background. Trade-off: a web build attempted in the first moments of a
# fresh container may race the install — cargo tests are unaffected (Rust is
# pre-installed), and once the container state is cached this finishes instantly.
echo '{"async": true, "asyncTimeout": 600000}'

# 1. WASM compile target for build_web.sh (no-op if already added).
rustup target add wasm32-unknown-unknown

# 2. wasm-pack. The prebuilt installer pulls a binary from GitHub releases,
#    which the web environment's proxy blocks, so build it from source via cargo
#    (cached after the first session). Only install when missing.
if ! command -v wasm-pack >/dev/null 2>&1; then
  cargo install wasm-pack --locked
fi

# 3. Warm the cargo dependency cache so the first build/test is fast and works
#    even if the network tightens mid-session.
cargo fetch --locked

# NOTE: `./build_web.sh` runs wasm-pack's wasm-opt size pass, which downloads
# binaryen from GitHub releases and is blocked by the proxy. That step is
# non-fatal — wasm-pack still emits pkg/glossia_bg.wasm and pkg/glossia.js
# before it runs, which is sufficient to serve and browser-verify web/. CI
# performs the optimized build for the deployed site.
