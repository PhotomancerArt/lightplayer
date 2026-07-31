#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/agent-context.sh [--planning-root PATH]

Prints shell-friendly agent context values:
  repo_root=...
  repo_slug=...
  planning_root=...
  repo_planning_root=...

The repo context is read from agent-context.toml when present. The planning
root resolves in this order, first match wins:
  1. --planning-root PATH
  2. the env var named by planning_root_env (default PHOTOMANCER_PLANNING_ROOT)
  3. planning_root from agent-context.toml
  4. ~/.photomancer/planning
USAGE
}

planning_root_override=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --planning-root)
      if [[ $# -lt 2 ]]; then
        printf 'Missing value for --planning-root\n' >&2
        exit 2
      fi
      planning_root_override="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

repo_root="$(git rev-parse --show-toplevel)"
context_file="$repo_root/agent-context.toml"

read_context_key() {
  local key="$1"

  if [[ ! -f "$context_file" ]]; then
    return 0
  fi

  sed -n "s/^[[:space:]]*$key[[:space:]]*=[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$context_file" |
    head -n 1
}

repo_slug="$(read_context_key repo_slug)"
planning_root_env="$(read_context_key planning_root_env)"
configured_planning_root="$(read_context_key planning_root)"

if [[ -z "$repo_slug" ]]; then
  repo_slug="$(basename "$repo_root")"
fi

if [[ -z "$planning_root_env" ]]; then
  planning_root_env="PHOTOMANCER_PLANNING_ROOT"
fi

planning_root="$planning_root_override"
if [[ -z "$planning_root" ]]; then
  planning_root="${!planning_root_env:-}"
fi
if [[ -z "$planning_root" && -n "$configured_planning_root" ]]; then
  planning_root="${configured_planning_root/#\~/$HOME}"
fi
if [[ -z "$planning_root" && ( -d "$HOME/.photomancer/planning" || -L "$HOME/.photomancer/planning" ) ]]; then
  planning_root="$HOME/.photomancer/planning"
fi

if [[ -z "$planning_root" ]]; then
  printf '%s is not set and ~/.photomancer/planning does not exist. Set it or pass --planning-root PATH.\n' "$planning_root_env" >&2
  exit 1
fi

if [[ ! -d "$planning_root" ]]; then
  printf 'Planning root does not exist: %s\n' "$planning_root" >&2
  exit 1
fi

repo_planning_root="$planning_root/$repo_slug"

printf 'repo_root=%q\n' "$repo_root"
printf 'repo_slug=%q\n' "$repo_slug"
printf 'planning_root_env=%q\n' "$planning_root_env"
printf 'planning_root=%q\n' "$planning_root"
printf 'repo_planning_root=%q\n' "$repo_planning_root"
