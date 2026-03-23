#!/usr/bin/env bash
set -euo pipefail

missing=0
warnings=0

ok() {
  echo "[OK] $1"
}

missing_dep() {
  echo "[MISSING] $1"
  missing=$((missing + 1))
}

warn() {
  echo "[WARN] $1"
  warnings=$((warnings + 1))
}

info() {
  echo "[INFO] $1"
}

if command -v rustc >/dev/null 2>&1; then
  ok "rustc $(rustc --version)"
else
  missing_dep "rustc (install Rust toolchain first)"
fi

if command -v cargo >/dev/null 2>&1; then
  ok "cargo $(cargo --version)"
else
  missing_dep "cargo (install Rust toolchain first)"
fi

if command -v cargo >/dev/null 2>&1 && cargo tauri --version >/dev/null 2>&1; then
  ok "cargo tauri $(cargo tauri --version)"
else
  missing_dep "tauri-cli (install with: cargo install tauri-cli --locked)"
fi

if command -v node >/dev/null 2>&1; then
  ok "node $(node --version)"
else
  missing_dep "node (install with: brew install node@22)"
fi

if command -v npm >/dev/null 2>&1; then
  ok "npm $(npm --version)"
else
  missing_dep "npm (comes with Node.js)"
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  if xcode-select -p >/dev/null 2>&1; then
    ok "Xcode Command Line Tools $(xcode-select -p)"
  else
    missing_dep "Xcode Command Line Tools (install with: xcode-select --install)"
  fi
fi

if command -v rustup >/dev/null 2>&1; then
  installed_targets="$(rustup target list --installed || true)"
  if echo "$installed_targets" | grep -q '^x86_64-unknown-uefi$'; then
    ok "Rust target x86_64-unknown-uefi (GO OS kernel)"
  else
    warn "Missing x86_64-unknown-uefi target. Add with: rustup target add x86_64-unknown-uefi"
  fi
fi

info "Tauri multi-OS build strategy:"
info "- macOS bundle: build on macOS"
info "- Linux bundle: build on Linux"
info "- Windows bundle: build on Windows"
info "Use CI matrix (GitHub Actions) for one source code base and multiple output OS bundles."

echo "[SUMMARY] missing=$missing warnings=$warnings"
if ((missing > 0)); then
  exit 2
fi
