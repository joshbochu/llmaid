# Changelog

All notable changes and the decisions behind them. Newest first.
Decision entries explain *why*, so future work doesn't relitigate them.

## [Unreleased]

### Added
- Layout/render (B12): boxes grow along the cross axis so each forward edge
  owns a distinct port — parallel edges keep separate paths and labels
  (no more last-label-wins collapse).
- Renderer: self-loops and cycle back edges now render as explicit return
  routes (B11), preserving arrows and labels instead of leaving disconnected
  stubs.
- Renderer: labeled edges reserve more horizontal breathing room around the
  label so arrows and text are easier to scan.
- Behavior contracts (`BEHAVIORS.md`, B1–B14) from a grilling session, with a
  given/when/then test layer (`tests/behavior.rs`) — one named test per landed
  contract; B9–B14 land with M2/M3. B6 immediately caught the placeholder CLI
  printing warnings to stdout (fixed: `dump_diagram` vs `dump`).
- Parser: `<br/>` in labels becomes a line break (B1); node redeclaration
  warns, last definition wins (B2).
- CLI: empty graph exits 0 with empty stdout + stderr warning (B7); default
  width fixed at 100, never terminal-detected (B8).
- **M1**: Mermaid flowchart parser (`parse.rs`) → IR: 7 shapes, all edge kinds
  (`-->`, `---`, `-.->`, `==>` + inline `-- text -->` and `|label|` forms),
  fan-out `&` groups, chained statements, forgiving directive handling.
  CLI skeleton (`main.rs`, std-only args: `--ascii --width --strict`) printing
  the IR dump until M2 rendering lands. Golden-snapshot harness
  (`tests/golden.rs` + `tests/cases/*.mmd` → `*.ir`, `UPDATE_GOLDEN=1` to
  regenerate) seeded with 10 cases incl. the session reference diagrams;
  error-quality and determinism tests.
- Cargo binary crate scaffold (edition 2024), `unicode-width` dependency.
- `DESIGN.md` (v1 design), `AGENTS.md` (agent guide), this changelog.

### Decisions

- **D1 — Input language: Mermaid flowchart subset.** Agents emit Mermaid
  fluently from training data (zero prompt budget); declarative DSL keeps
  geometry in the tool where it belongs. Alternatives rejected: Python API
  (execution sandbox, imperative failure modes), novel DSL (no training data,
  syntax must be taught in every prompt).

- **D2 — Language: Rust.** ~1ms startup from a single static binary (agents
  call the tool many times per response), canonical `unicode-width` crate,
  clean wasm path for future npm/browser embedding. Go was close (autog layout
  lib, mmgo mermaid parsers) but loses on wasm, width-measurement maturity,
  and no Go layout lib promises deterministic output. Python rejected:
  100–300ms interpreter startup contradicts "extremely fast".

- **D3 — Own grid-native layout; no dagre/rust-sugiyama crates.** Character
  grids want integer coordinates; float→grid snapping is where alignment bugs
  breed (root cause of termiflow's broken output). Sugiyama phases (rank /
  order / position) are classic and small at our scale. Fallback if quality
  stalls: `dagre` crate (full dagre.js port) behind the same narrow layout API.

- **D4 — Only dependency: `unicode-width`.** Terminal column measurement is a
  maintained-Unicode-tables problem (CJK, emoji ZWJ, combining marks; shifts
  with each Unicode release) — exactly what to outsource. Canonical unicode-rs
  crate used by ratatui et al. Rolling our own rejected as re-deriving the same
  tables, worse. Known limit (all tools share it): chat UIs / markdown viewers
  with font fallback may visually wobble non-ASCII labels; real terminals align.

- **D5 — No CLI framework; std-only arg parsing.** Five flags (`--ascii`,
  `--width`, `--strict`, `--help`, `--version`) ≈ 40 lines by hand. clap
  rejected: ~10 transitive deps + compile time against a minimal-binary thesis.
  Escalation path if the surface grows: `lexopt` (zero-dep) before clap.

- **D6 — Name: `llmaid`.** "LLM aid" / mermaid pun. Verified unclaimed on
  crates.io and npm (PyPI `llmaid` exists but is an unrelated LLM wrapper, and
  we don't target PyPI). `termaid` rejected: existing Python Mermaid renderer.

- **D7 — Aesthetic defaults are the spec** (not themeable in v1): rounded
  corners, thin lines, `▶` arrowheads, labels on the arrow, never truncate.
  One good default over a flag zoo; determinism (same bytes) is a feature.

- **D8 — v1 scope: flowcharts only.** LR/RL/TB/BT, 7 shapes, edge labels,
  fork/merge, cycles, self-loops. Sequence diagrams and trees are v2 (diagon
  remains the stopgap for those). Subgraphs and `classDef`/`style` directives
  parsed-and-ignored, never errors.

- **D9 — Overflow ladder, never truncation or failure.** When a diagram
  exceeds the width budget: compact inter-node gaps → wrap labels → if still
  too wide, render over-width anyway. Rejected: hard error (agents can't fix a
  diagram's intrinsic width), truncation (violates D7). Labels wrap *only*
  under width pressure — no arbitrary box-width cap.

- **D10 — stdout purity.** stdout carries only the diagram; every diagnostic
  goes to stderr. Agents pipe output straight into PR comments/chat; a warning
  in the diagram body is corruption. Rejected: footer warnings in stdout.

- **D11 — Fixed default width (100), no TTY detection.** Terminal-width
  detection makes identical invocations produce different bytes in different
  terminals, silently breaking the determinism promise (D7). `--width` is the
  only way to change it.

- **D12 — Empty graph is trivia, not an error.** Exit 0, empty stdout,
  warning on stderr. Pipelines must not break because an agent emitted a bare
  header or a comment-only file.

- **D13 — Shape hints over true shape outlines.** Non-rect shapes render as
  rect-framed boxes with hint glyphs (◇ corners for diamond, rounded caps for
  stadium/circle, cylinder lid) rather than true outlines. True diamond walls
  are exactly where termiflow's alignment broke; grid discipline outranks
  shape fidelity in v1.

- **D14 — Behavior contracts as a first-class test layer.** `BEHAVIORS.md`
  indexes numbered given/when/then contracts; each has a matching
  `b<N>_given_..._then_...` test in `tests/behavior.rs` (CLI contracts run the
  real binary). Rejected: cucumber-rs (dev-dependency for no added rigor),
  goldens-only (snapshots show *what*, not *why*; contracts survive renderer
  rewrites). Other grilled parser rulings: `<br/>` honored as line break (B1),
  node redeclaration warns + last wins (B2), parallel edges kept with both
  labels (B3, rendered per D9 as distinct paths).
