#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LITEHTML_DIR="${ROOT_DIR}/kernel/third_party/litehtml"
UPSTREAM_DIR="${LITEHTML_DIR}/upstream"
UPSTREAM_REPO="https://github.com/litehtml/litehtml.git"
COMMIT_FILE="${LITEHTML_DIR}/UPSTREAM_COMMIT.txt"

mkdir -p "${LITEHTML_DIR}"

if [ ! -d "${UPSTREAM_DIR}/.git" ]; then
  echo "[litehtml] cloning upstream..."
  git clone --depth 1 "${UPSTREAM_REPO}" "${UPSTREAM_DIR}"
else
  echo "[litehtml] updating upstream..."
  git -C "${UPSTREAM_DIR}" fetch --depth 1 origin HEAD
  git -C "${UPSTREAM_DIR}" reset --hard FETCH_HEAD
fi

UPSTREAM_COMMIT="$(git -C "${UPSTREAM_DIR}" rev-parse HEAD)"
printf '%s\n' "${UPSTREAM_COMMIT}" > "${COMMIT_FILE}"

echo "[litehtml] upstream commit: ${UPSTREAM_COMMIT}"
echo "[litehtml] path: ${UPSTREAM_DIR}"
