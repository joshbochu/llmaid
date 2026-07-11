#!/usr/bin/env bash
# Print every golden case: source + render.
#
#   ./scripts/show-gallery.sh              # live render
#   ./scripts/show-gallery.sh --txt        # committed *.txt snapshots (fast)
#   ./scripts/show-gallery.sh pipeline     # filter by name substring
#   ./scripts/show-gallery.sh --txt cycle  # combine flags
#
# For a multi-diagram packed contact sheet (side-by-side shelves), see:
#   ./scripts/contact-sheet.py --help
#
# Run each command on its own line (don't paste comments on the same line as the command).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASES="$ROOT/tests/cases"
MODE=live
FILTER=""

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \?//'
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    --txt) MODE=txt; shift ;;
    --live) MODE=live; shift ;;
    -*)
      echo "unknown option: $1 (try --help)" >&2
      exit 2
      ;;
    *)
      FILTER="$1"
      shift
      ;;
  esac
done

if [[ ! -d "$CASES" ]]; then
  echo "cases dir not found: $CASES" >&2
  exit 1
fi

# Collect case names without relying on shell glob (zsh-safe).
names=()
while IFS= read -r -d '' f; do
  n="$(basename "$f" .mmd)"
  if [[ -n "$FILTER" && "$n" != *"$FILTER"* ]]; then
    continue
  fi
  names+=("$n")
done < <(find "$CASES" -maxdepth 1 -type f -name '*.mmd' -print0 | sort -z)

if [[ ${#names[@]} -eq 0 ]]; then
  echo "no cases matched (dir=$CASES filter=${FILTER:-none})" >&2
  echo "available:" >&2
  find "$CASES" -maxdepth 1 -type f -name '*.mmd' -exec basename {} .mmd \; | sort >&2
  exit 1
fi

resolve_bin() {
  if [[ -x "$ROOT/target/release/llmaid" ]]; then
    printf '%s\n' "$ROOT/target/release/llmaid"
  elif [[ -x "$ROOT/target/debug/llmaid" ]]; then
    printf '%s\n' "$ROOT/target/debug/llmaid"
  else
    (cd "$ROOT" && cargo build -q)
    printf '%s\n' "$ROOT/target/debug/llmaid"
  fi
}

LLMAID=""
if [[ "$MODE" == "live" ]]; then
  LLMAID="$(resolve_bin)"
fi

for n in "${names[@]}"; do
  printf '%s\n' "════════════════════════════════════════════════════════════"
  printf '%s\n' " CASE: $n"
  printf '%s\n' "════════════════════════════════════════════════════════════"
  printf '%s\n' "── source ($n.mmd) ──"
  cat "$CASES/$n.mmd"
  printf '\n%s\n' "── render ──"
  if [[ "$MODE" == "txt" ]]; then
    if [[ -f "$CASES/$n.txt" ]]; then
      cat "$CASES/$n.txt"
    else
      printf '%s\n' "(no $n.txt snapshot)"
    fi
  else
    "$LLMAID" "$CASES/$n.mmd" 2>/dev/null || "$LLMAID" "$CASES/$n.mmd" 2>&1
  fi
  printf '\n'
done

printf '%s\n' "════════════════════════════════════════════════════════════"
printf '%s\n' " ${#names[@]} case(s). Byte-compare: cargo test --test golden"
printf '%s\n' " Regen after intentional edits: UPDATE_GOLDEN=1 cargo test"
printf '%s\n' "════════════════════════════════════════════════════════════"
