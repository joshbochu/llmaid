# Handoff — llmaid

Last updated: 2026-07-06, end of first session.

## What this project is

`llmaid` renders Mermaid flowcharts as clean, deterministic Unicode diagrams in
the terminal. Target user: coding agents composing diagrams into their output.
Thesis: diagon's alignment correctness + termiflow's aesthetic (rounded,
spacious, minimalist), behind Mermaid syntax, in one fast Rust binary.

Read these before writing code, in this order:

1. `AGENTS.md` — commands, module map, non-negotiable invariants, conventions.
2. `DESIGN.md` — full v1 design: scope, architecture, aesthetic spec, milestones.
3. `BEHAVIORS.md` — behavior contracts B1–B14; B9–B14 are *pending* and land
   with M2/M3 (each needs its `b<N>_...` test when implemented).
4. `CHANGELOG.md` — decisions D1–D14 with rationale and rejected alternatives.
   Do not relitigate these; extend the log if you make new decisions.

## Current state (exact)

**Milestone M1 is complete and green. M2 is ~40% written and NOT yet compiling
as a unit — see "M2 in flight" below.**

- `cargo test`: 13/13 pass (10 behavior + 3 golden/error/determinism).
  NOTE: tests currently compile because `src/lib.rs` only declares
  `pub mod parse;` — the new M2 files are not yet registered as modules and
  have therefore NEVER been compiled. Expect compile errors on first build.
- **Nothing is committed yet.** All files untracked; git repo initialized with
  no commits. The user has not asked for commits — ask before committing.
- Sandbox note: cargo builds default to a shared cache target dir; use
  `CARGO_TARGET_DIR=$PWD/target` when you need `./target/release/llmaid` to
  exist in-workspace.

### Done (M1 + grilled behaviors)

- `src/parse.rs` (583 lines) — full v1 syntax: 7 shapes, `-->`/`---`/`-.->`/
  `==>` + `-- text -->` and `|label|` forms, `&` fan-out, chaining, `;`
  statements, `%%` comments, quoted labels, `<br/>` → newline in labels (B1),
  redeclaration warns + last wins (B2), forgiving directives (B5), errors with
  line + expectation (B4). `dump()` = golden format (includes warnings);
  `dump_diagram()` = stdout format (no warnings — B6).
- `src/main.rs` (133 lines) — std-only CLI: `--ascii --width --strict -h -V`,
  file-or-stdin. stdout purity (B6), empty graph → exit 0 + stderr warning
  (B7), fixed default width 100 (B8). Currently prints the IR dump as a
  placeholder — M2 replaces that with the render.
- `tests/golden.rs` + `tests/cases/*.mmd`→`*.ir` (10 cases) — regenerate with
  `UPDATE_GOLDEN=1 cargo test`. Cases include the user's reference diagrams
  (pipeline, diamond, forkmerge, cycle w/ self-loop) — these are the quality
  bar for rendering; the user picked their style preferences from these.
- `tests/behavior.rs` — b1..b8 given/when/then tests; CLI ones run the real
  binary via `CARGO_BIN_EXE_llmaid`.

### M2 in flight (written this session, unverified)

- `src/style.rs` (89 lines) — Unicode/ASCII glyph sets; junction resolution by
  N/E/S/W bitmask (`─` meets `│` ⇒ `┼`); rounded corners; dotted `┄┊`, thick
  `━┃`; arrowheads `▶◀▲▼` / `>< ^v`.
- `src/layout.rs` (686 lines) — the Sugiyama implementation in integer grid
  coords. Key abstraction: **flow space** — `f` along the rank axis, `c`
  across; LR/RL map flow→screen-x, TB/BT flow→screen-y, RL/BT mirror the flow
  axis at render time. One layout implementation serves all four directions.
  Pipeline: DFS back-edge marking (declaration order) → longest-path ranks
  (Kahn) → dummy nodes for multi-rank edges → 4 barycenter sweeps (ties by
  declaration index) → cross-coordinate legalization sweeps → port assignment
  (edges spread across a box side's interior rows, ordered by far-end cross
  position) → channel segments between adjacent ranks with a label zone +
  numbered jog tracks (`Channel::track_f`). Self-loops and back edges are
  classified and left for route/render (B11: tight side loop; back edges:
  perimeter channel).

### Not started

- `src/render.rs` — canvas + box drawing + shape hints (B13: rect frame with
  hint glyphs, e.g. ◇ corners for diamond — see DESIGN.md aesthetic spec) +
  consuming `layout::Placed` (boxes, segs, channels, pass_through) to draw
  edges, elbows, on-arrow labels, arrowheads, self-loops, back edges.
- Registering modules in `src/lib.rs` (`pub mod layout; pub mod style;` +
  render when it exists) and wiring `main.rs`: parse → layout → render,
  honoring `--ascii` and `--width`.
- `.txt` golden snapshots for rendered output; invariant checks (B14);
  behavior tests b9–b14; the D9 overflow ladder (compact → wrap → over-width);
  `--width` is currently accepted but unused.

## Suggested next steps, in order

1. Add `pub mod layout; pub mod style;` to `lib.rs`, run `cargo build`, fix
   compile errors in `layout.rs` (it has never compiled; expect borrow-checker
   friction in `order_by_barycenter`'s `split_at_mut` usage and the
   `legalize`/`neighbor_centers` call in the alignment sweep).
2. Write a minimal `render.rs` for LR only: boxes + straight/elbow channel
   edges + arrowheads, no labels. Wire into main. Eyeball
   `tests/cases/pipeline.mmd` — get first light before adding features.
3. Iterate: on-arrow edge labels (space already reserved via channel
   `label_zone`), fork/merge (diamond case), TB, then RL/BT mirroring.
4. Then: shape hints (B13), self-loops (B11), back edges, `--ascii`,
   invariants (B14), `.txt` goldens, remaining behavior tests, overflow ladder
   (B9/B10), parallel edges (B12).
5. Keep `CHANGELOG.md` and `BEHAVIORS.md` current as behaviors land — this is
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
