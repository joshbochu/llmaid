# Handoff — llmaid

Last updated: 2026-07-09, after committing M2 renderer + B11.

## What this project is

`llmaid` renders Mermaid flowcharts as clean, deterministic Unicode diagrams in
the terminal. Target user: coding agents composing diagrams into their output.
Thesis: diagon's alignment correctness + termiflow's aesthetic (rounded,
spacious, minimalist), behind Mermaid syntax, in one fast Rust binary.

Read these before writing code, in this order:

1. `AGENTS.md` — commands, module map, non-negotiable invariants, conventions.
2. `DESIGN.md` — full v1 design: scope, architecture, aesthetic spec, milestones.
3. `BEHAVIORS.md` — behavior contracts B1–B14; B9–B10 and B12–B14 are
   *pending* and land with M2/M3 (each needs its `b<N>_...` test when
   implemented).
4. `CHANGELOG.md` — decisions D1–D14 with rationale and rejected alternatives.
   Do not relitigate these; extend the log if you make new decisions.

## Current state (exact)

**M2 renderer + B11 are committed on `main`.** Working tree should be clean.

- Baseline includes parse → layout → render end-to-end; layout/style wired in
  `lib.rs`; CLI honors `--ascii` (`--width` still unused).
- `cargo test`: 14/14 pass (11 behavior + 3 golden/error/determinism).
- `src/render.rs` draws boxes, forward channel edges, arrowheads, on-arrow
  labels for LR/RL, ASCII/Unicode glyphs, self-loops, and cycle back edges.
- `tests/cases/*.txt` rendered snapshots are in the tree for eyeballing; they
  are not yet byte-compared (B14).

### Done

- Parser behaviors B1–B5 and CLI behaviors B6–B8 are still green.
- Minimal M2 pipeline is wired: `pipeline.mmd` renders at the quality bar.
- LR/TB layouts, fork/merge, fanout, dotted/thick edge styles, Unicode labels,
  and `--ascii` are working in the current renderer.
- B11 is landed: self-loops hug the node and cycle back edges use perimeter
  return routes with arrows/labels preserved. `tests/behavior.rs` includes
  `b11_given_self_loop_and_back_edge_then_routes_return_to_targets`.

### Still missing

- B9/B10: `--width` overflow ladder (compact gaps → wrap labels → over-width
  rather than truncating/failing).
- B12: parallel edges need fully distinct paths and labels; `edge-labels.mmd`
  is still the main quality bar.
- B13: shape hints for non-rect shapes (diamond/cylinder/stadium/circle/etc.).
- B14: rendered `.txt` golden comparison plus invariants (closed borders,
  edges reach endpoints, no label overwrite).
- Behavior tests b9, b10, b12, b13, b14 and corresponding pending-marker
  removals in `BEHAVIORS.md`.
- RL/BT mirroring needs explicit verification beyond compile/test coverage.

## Suggested next steps, in order

1. B12 parallel routing: make `edge-labels.mmd` show each A↔B/C path and label
   distinctly without merged thick/dotted spans.
2. B13 shape hints: rect frame + hint glyphs per DESIGN.md.
3. B9/B10 width ladder: wire the fixed default width through layout/render.
4. B14 rendered goldens + invariants: byte-compare `tests/cases/*.txt` and add
   border/endpoint/label overwrite checks.
5. Verify RL/BT mirroring with explicit cases.
6. Keep `CHANGELOG.md` and `BEHAVIORS.md` current as behaviors land — this is
   a logged convention in AGENTS.md.

## Quality bar (what "done" looks like)

The user's reference for taste — output should look like:

```
╭─────────╮  scan   ╭──────────────╮  parse   ╭──────────╮
│ source  ├────────▶│  Vec<Token>  ├─────────▶│ Expr AST │
╰─────────╯         ╰──────────────╯          ╰──────────╯
```

Rounded corners, labels sitting on the arrow, generous-but-consistent gaps,
nothing truncated, nothing floating. When in doubt render `tests/cases/*.mmd`
and eyeball against the aesthetic spec in DESIGN.md. The failure modes to
avoid are termiflow's: truncated labels (`pr…`), floating edge labels,
detached shape glyphs.

## Context that is easy to lose

- The user cares most about: minimalist readable output, correct alignment,
  speed. They explicitly dislike dense/heavy table-style output.
- Comparison tools installed locally for reference renders: `tw` (termiflow,
  `~/.cargo/bin/tw`), diagon wrapper (`node ~/dev/lox-rs/tools/diagon.mjs`),
  graph-easy (`PERL5LIB=~/perl5/lib/perl5 ~/perl5/bin/graph-easy`), and
  `uvx termaid` (the PyPI competitor; also why our name isn't termaid).
- Determinism is a hard invariant (D7/B8): no HashMap iteration, no terminal
  detection, stable tie-breaks everywhere. If you add an ordering, break ties
  by declaration index.
- v1 scope is flowcharts only (D8). Don't start sequence diagrams.
