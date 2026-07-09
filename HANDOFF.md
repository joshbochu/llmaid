# Handoff — llmaid

Last updated: 2026-07-09, after B14 rendered goldens + invariants.

## What this project is

`llmaid` renders Mermaid flowcharts as clean, deterministic Unicode diagrams in
the terminal. Target user: coding agents composing diagrams into their output.
Thesis: diagon's alignment correctness + termiflow's aesthetic (rounded,
spacious, minimalist), behind Mermaid syntax, in one fast Rust binary.

Read these before writing code, in this order:

1. `AGENTS.md` — commands, module map, non-negotiable invariants, conventions.
2. `DESIGN.md` — full v1 design: scope, architecture, aesthetic spec, milestones.
3. `BEHAVIORS.md` — behavior contracts B1–B14 (all landed).
4. `CHANGELOG.md` — decisions D1–D14 with rationale and rejected alternatives.
   Do not relitigate these; extend the log if you make new decisions.

## Current state (exact)

**v1 behavior contracts B1–B14 are landed on `main`.** Working tree should be clean.

- Full pipeline: parse → layout (width ladder) → render (shape hints, cycles,
  parallel ports).
- `cargo test`: behavior suite + IR/render goldens + B14 canvas invariants.
- CLI: `--ascii`, `--width N` (default 100), `--strict`, file or stdin.
- Regenerate goldens: `UPDATE_GOLDEN=1 cargo test` (only when output is better).

### Done

- B1–B8: parse + CLI contracts.
- B9/B10: `--width` overflow ladder (compact → wrap → over-width).
- B11: self-loops and back-edge perimeter routes.
- B12: distinct ports for parallel edges.
- B13: rect-framed shape hints (◇ ( ) ═ ╱╲).
- B14: `.txt` goldens + `render_with_checks` invariants.

### Still open (post-contract polish)

- Explicit RL/BT verification cases (mirroring is implemented; light coverage).
- Multi-rank dummy conflicts for rare parallel long edges.
- Edge-label placement for TB (on-arrow labels are LR-focused today).
- Optional: tighten channel/jog aesthetics on `edge-labels.mmd`.

## Suggested next steps, in order

1. Explicit RL/BT golden cases.
2. TB edge labels on the vertical run when space allows.
3. Any aesthetic pass against reference tools (termiflow/diagon).
4. Keep `CHANGELOG.md` and `BEHAVIORS.md` current as behaviors land.

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
