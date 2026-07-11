# Handoff — llmaid

Last updated: 2026-07-11 — core sequence vertical slice landed.

## What this project is

`llmaid` renders Mermaid as clean, deterministic Unicode diagrams in the
terminal. **Agents create and self-debug; humans look at the visuals.**

Thesis: diagon's alignment correctness + termiflow's aesthetic, behind Mermaid
syntax, in one fast Rust binary — expanding toward multi-type Mermaid breadth
without losing glance quality.

## Read order

1. `AGENTS.md` — commands, modules, invariants, conventions  
2. `DESIGN.md` — v1 design thesis and architecture  
3. `ROADMAP.md` — phased plan (what to build next)  
4. `MATRIX.md` — capability × tool coverage checklist  
5. `BEHAVIORS.md` — shipped contracts B1–B17
6. `CHANGELOG.md` — decisions D1–D18

## Current state

**Contracts B1–B17 + Phase 0–1 + the Phase 2 core slice are landed.**

- Pipeline: parse → flow layout → route into signed `Scene` → pure canvas render
- Diagram dispatch: flowcharts retain their established pipeline; the sequence
  engine owns its semantic IR/layout and joins at the shared `Scene` boundary
- Core sequence diagrams: declared/implicit participants and actors, padded
  headers, dotted lifelines, `->>` messages, and `-->>` returns
- Sequence visual grammar: calls end with a filled arrow at the destination
  lifeline (`────▶┊`); returns begin at the destination with a thin arrow
  (`┊←────`); ASCII uses `-->|` and `|<--`
- `Scene` owns complete paths, arrows, label positions, normalization, and exact bounds
- **Subgraphs:** titled frames around members (B15); interior title band +
  spacious pad; nested parent tracked
- Edge labels: padded on-shaft (` scan `); TB/BT labels beside vertical runs
- Directions: LR/RL/TB/BT with goldens
- CLI: `--ascii`, `--width N` (default 100), `--strict`
- Tests: behavior + IR/render goldens + B14/B16 scene invariants + exact
  topology-aware quality contracts (`tests/quality.rs`)
- Quality audit: `cargo run -q --example symmetry` reports hard failures,
  doubled-cell relational residuals, crossings, bends, and wire length
- Guarantee boundary: engine-wide grid/determinism rules and scene invariants
  are distinct from topology-specific aesthetic contracts. Known chains,
  forks, merges, eligible diamonds, and group boundaries have exact tests;
  goldens do not prove arbitrary unclassified topologies beautiful. See
  `DESIGN.md` "Quality guarantee model."
- Vertical routing widens lone distinct-peer junction boxes to preserve straight
  attachment shafts; grouped external children start after internal content;
  labeled rank gaps place their label on the exact middle row
- Horizontal acyclic, non-reconverging forks may grow across child rows; box
  labels are centered vertically after routing-driven growth
- Equal-width vertical chains resolve odd/even text parity with a deterministic
  right bias toward the shared arrow column; flipped chains choose width parity
  from the terminal/top label; root forks keep a two-cell port margin
- Visual review: `./scripts/review-gallery.py --serve` opens the all-case bulk
  annotation workflow and atomically saves `.llmaid-review.json`. Its explicit
  terminal-cell painter keeps CJK/emoji alignment representative in a browser.
  The terminal slideshow (`./scripts/review-gallery.py`, optionally `--live`)
  remains the final font/glyph-fidelity check for flagged cases.
- Regenerate goldens: `UPDATE_GOLDEN=1 cargo test`

## Next steps (from ROADMAP)

1. **Phase 2.3** — sequence notes and activate/deactivate
2. **Quality self-debug** — expose named geometry violations through
   `--audit=json`, then add generated/metamorphic small-graph coverage
3. Sequence control blocks (`loop` / `alt` / `opt`) after the core additions
4. Design types → planning → charts → broader agent diagnostics → distribute
5. Optional: nested subgraph polish / exit-edge routing through frames

Track detail in `ROADMAP.md`; tick coverage in `MATRIX.md`.

### Suggested Phase 2.3 pickup

Land another thin vertical slice: parse and render `Note left of`, `Note right
of`, and `Note over` first, then add `activate` / `deactivate` bars. Start with
the numbered behavior contract in `tests/behavior.rs`, add sequence structural
tests plus `.mmd` / `.txt` goldens, and keep all new geometry in
`sequence.rs` emitting shared `Scene` primitives. Do not fold sequence
semantics into the flowchart `Graph`.

## Quality bar

```
╭────────╮             ╭────────────╮              ╭──────────╮
│ source ├─── scan ───▶│ Vec<Token> ├─── parse ───▶│ Expr AST │
╰────────╯             ╰────────────╯              ╰──────────╯
```

Rounded, labels on the arrow with spaces, nothing truncated. Eyeball
`tests/cases/*.mmd` when unsure.

## Local comparison tools

- termiflow: `tw` (`~/.cargo/bin/tw`)  
- diagon: `node ~/dev/lox-rs/tools/diagon.mjs <seq|tree|math|table|frame|dag|grammar>`  
- graph-easy: `PERL5LIB=~/perl5/lib/perl5 ~/perl5/bin/graph-easy`  
- termaid: `uvx --from termaid termaid`  
