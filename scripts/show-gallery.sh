#!/usr/bin/env bash
# Print every golden case: source + live render (or committed .txt if --txt).
# Usage:
#   ./scripts/show-gallery.sh           # live render via cargo run
#   ./scripts/show-gallery.sh --txt     # committed snapshots only (fast)
#   ./scripts/show-gallery.sh pipeline  # one case by name
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
CASES="$ROOT/tests/cases"
MODE=live
FILTER="${1:-}"

if [[ "${1:-}" == "--txt" ]]; then
  MODE=txt
  FILTER="${2:-}"
elif [[ "${2:-}" == "--txt" ]]; then
  MODE=txt
fi

bin() {
  if [[ -x "$ROOT/target/release/llmaid" ]]; then
    echo "$ROOT/target/release/llmaid"
  elif [[ -x "$ROOT/target/debug/llmaid" ]]; then
    echo "$ROOT/target/debug/llmaid"
  else
    cargo build -q
    echo "$ROOT/target/debug/llmaid"
  fi
}

LLMAID="$(bin)"
names=()
for f in "$CASES"/*.mmd; do
  n="$(basename "$f" .mmd)"
  if [[ -n "$FILTER" && "$FILTER" != "--txt" && "$n" != *"$FILTER"* ]]; then
    continue
  fi
  names+=("$n")
done

if [[ ${#names[@]} -eq 0 ]]; then
  echo "no cases matched" >&2
  exit 1
fi

for n in "${names[@]}"; do
  echo "════════════════════════════════════════════════════════════"
  echo " CASE: $n"
  echo "════════════════════════════════════════════════════════════"
  echo "── source ($n.mmd) ──"
  cat "$CASES/$n.mmd"
  echo
  echo "── render ──"
  if [[ "$MODE" == "txt" ]]; then
    if [[ -f "$CASES/$n.txt" ]]; then
      cat "$CASES/$n.txt"
    else
      echo "(no $n.txt snapshot)"
    fi
  else
    "$LLMAID" "$CASES/$n.mmd" 2>/dev/null || "$LLMAID" "$CASES/$n.mmd" 2>&1
  fi
  echo
done

echo "════════════════════════════════════════════════════════════"
echo " ${#names[@]} case(s). Byte-compare goldens: cargo test --test golden"
echo " Regenerate .txt/.ir after intentional changes: UPDATE_GOLDEN=1 cargo test"
echo "════════════════════════════════════════════════════════════"
