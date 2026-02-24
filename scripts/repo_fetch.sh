#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOWNLOADS_DIR="${PROJECT_ROOT}/Downloads"

usage() {
  cat <<'EOF'
Uso:
  scripts/repo_fetch.sh search "<consulta>" [limite]
  scripts/repo_fetch.sh download <owner/repo|url_git>

Ejemplos:
  scripts/repo_fetch.sh search "terminal file manager" 10
  scripts/repo_fetch.sh download sharkdp/bat
  scripts/repo_fetch.sh download https://github.com/sharkdp/bat.git
EOF
}

search_with_gh() {
  local query="$1"
  local limit="$2"
  gh repo search "$query" --limit "$limit" --sort stars
}

search_with_api() {
  local query="$1"
  local limit="$2"
  curl -fsSL -G "https://api.github.com/search/repositories" \
    --data-urlencode "q=${query}" \
    --data-urlencode "sort=stars" \
    --data-urlencode "order=desc" \
    --data-urlencode "per_page=${limit}" |
    jq -r '.items[] | "\(.full_name) | ⭐ \(.stargazers_count)\n\(.html_url)\n\(.description // "")\n"'
}

search_repos() {
  local query="${1:-}"
  local limit="${2:-10}"

  if [[ -z "$query" ]]; then
    echo "Error: falta la consulta."
    usage
    exit 1
  fi

  if command -v gh >/dev/null 2>&1; then
    echo "Buscando en GitHub con gh..."
    search_with_gh "$query" "$limit"
    return
  fi

  if command -v curl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
    echo "Buscando en GitHub con API publica..."
    search_with_api "$query" "$limit"
    return
  fi

  echo "No encontre 'gh' ni la combinacion 'curl + jq'."
  echo "Instala GitHub CLI o jq para poder buscar repositorios."
  exit 1
}

download_repo() {
  local input="${1:-}"
  local clone_url repo_name target_dir

  if [[ -z "$input" ]]; then
    echo "Error: falta el repositorio a descargar."
    usage
    exit 1
  fi

  if [[ "$input" =~ ^https?:// ]]; then
    clone_url="$input"
    repo_name="$(basename "${input%.git}")"
  else
    clone_url="https://github.com/${input}.git"
    repo_name="$(basename "$input")"
  fi

  mkdir -p "$DOWNLOADS_DIR"
  target_dir="${DOWNLOADS_DIR}/${repo_name}"

  if [[ -e "$target_dir" ]]; then
    echo "Ya existe: $target_dir"
    echo "Borra o renombra esa carpeta e intenta de nuevo."
    exit 1
  fi

  echo "Clonando:"
  echo "  $clone_url"
  echo "en:"
  echo "  $target_dir"
  git clone "$clone_url" "$target_dir"
  echo "Listo."
}

main() {
  local command="${1:-}"

  case "$command" in
    search)
      shift || true
      search_repos "${1:-}" "${2:-10}"
      ;;
    download)
      shift || true
      download_repo "${1:-}"
      ;;
    *)
      usage
      exit 1
      ;;
  esac
}

main "$@"
