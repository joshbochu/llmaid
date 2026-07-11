# Handoff — llmaid

Last updated: 2026-07-11 — audit, generated coverage, and sequence controls landed.

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
5. `BEHAVIORS.md` — shipped contracts B1–B20
6. `CHANGELOG.md` — decisions D1–D21

## Current state

**Contracts B1–B20 + Phase 0–2 core + Phase 6.2–6.3 are landed.**

- Pipeline: parse → flow layout → route into signed `Scene` → pure canvas render
- Diagram dispatch: flowcharts retain their established pipeline; the sequence
  engine owns its semantic IR/layout and joins at the shared `Scene` boundary
- Core sequence diagrams: declared/implicit participants and actors, padded
  headers, dotted lifelines, ordered messages/notes/activation events, `->>`
  messages, `-->>` returns, left/right/over notes, balanced activation bars,
  and nested framed `loop`, `alt` / `else`, and `opt` controls
- Sequence visual grammar: calls end with a filled arrow at the destination
  lifeline (`────▶┊`); returns begin at the destination with a thin arrow
  (`┊←────`); active messages attach to the nearest bar boundary; ASCII uses
  `-->|` and `|<--`
- `Scene` owns complete paths, arrows, label positions, normalization, and exact bounds
- **Subgraphs:** titled frames around members (B15); interior title band +
  spacious pad; nested parent tracked
- Edge labels: padded on-shaft (` scan `); TB/BT labels beside vertical runs
- Directions: LR/RL/TB/BT with goldens
- CLI: `--ascii`, `--width N` (default 100), `--strict`, `--audit=json`
- Tests: behavior + IR/render goldens + B14/B16 scene invariants + exact
  topology-aware quality contracts (`tests/quality.rs`)
- Quality audit: `cargo run -q --example symmetry` reports hard failures,
  doubled-cell relational residuals, crossings, bends, and wire length
- CLI audit: `--audit=json` emits byte-stable `llmaid.audit.v1` for flowcharts
  and sequences with normalized bounds/counts, named invariant violations,
  exact edge/box/cell witnesses, and the flowchart metric vector
- Generated coverage: all 71 non-empty forward DAGs on 2–4 nodes render in all
  four directions (284 cases); opposite directions have exact audit signatures
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

1. **Phase 3.1** — flat `stateDiagram` / `stateDiagram-v2`
2. `classDiagram`, then `erDiagram`
3. Planning/hierarchy → charts → remaining agent diagnostics → distribute
4. Optional: nested subgraph polish / exit-edge routing through frames

Track detail in `ROADMAP.md`; tick coverage in `MATRIX.md`.

### Suggested next pickup

Land a thin flat-state vertical slice: detect `stateDiagram` /
`stateDiagram-v2`, parse named states and transitions into a type-specific IR,
emit the shared `Scene`, and ship behavior/error coverage plus focused goldens
before considering nested composite states.

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
