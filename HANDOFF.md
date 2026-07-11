# Handoff — llmaid

Last updated: 2026-07-11 — Phase 4.1 core mindmap hierarchy landed.

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
5. `BEHAVIORS.md` — shipped contracts B1–B25
6. `CHANGELOG.md` — decisions D1–D25

## Current state

**Contracts B1–B25 + Phase 0–3 core + Phase 4.1 + Phase 6.2–6.3/6.5 are landed.**

- Pipeline: parse → flow layout → route into signed `Scene` → pure canvas render
- Diagram dispatch: flowcharts retain their established pipeline; the sequence
  engine owns its semantic IR/layout and joins at the shared `Scene` boundary
- Core sequence diagrams: declared/implicit participants and actors, padded
  headers, dotted lifelines, ordered messages/notes/activation events, `->>`
  messages, `-->>` returns, left/right/over notes, balanced activation bars,
  and nested framed `loop`, `alt` / `else`, and `opt` controls
- Core design-doc diagrams: flat state machines with aliases/markers/labeled
  transitions; classes with members, UML relation operators, multiplicities,
  and labels; ER entities with typed attributes, PK/FK/UK markers, comments,
  cardinalities, and identifying/non-identifying relations
- Core mindmaps: `mindmap` dispatch, one two-space-indented root, plain
  descendants, stable sibling order, canonical `root((label))` label support,
  and distinct line-specific errors for malformed indentation, missing parents,
  multiple roots, deferred syntax, and zero-width terminal sequences
- Reusable tree geometry: independent integer-grid ordered-tree layout with
  left-to-right depth columns, exact parent/child-span centering, shared
  arrowless trunks, Unicode measurement, width fallback, and shared-Scene output
- Typed boxed adapter: state/class/ER retain independent semantic IR and join
  the established integer layered geometry only when lowering to `Scene`
- Design-doc visual grammar: class headers/members use compartments; UML
  aggregation/composition/inheritance/realization semantics use endpoint
  diamonds/arrows/triangles; ER attributes use aligned table rows/columns and
  zero/one/many glyphs attach at relationship endpoints in Unicode and ASCII.
  Layout reserves one connector cell between boxes and adornments and between
  paired cardinality marks, matching the accepted Phase 3.4 gallery review
- Sequence visual grammar: calls end with a filled arrow at the destination
  lifeline (`────▶┊`); returns begin at the destination with a thin arrow
  (`┊←────`); active messages attach to the nearest bar boundary; ASCII uses
  `-->|` and `|<--`
- `Scene` owns complete paths, arrows, label positions, normalization, and exact bounds
- **Subgraphs:** titled frames around members (B15); interior title band +
  spacious pad; nested parent tracked
- Edge labels: padded on-shaft (` scan `); TB/BT labels beside vertical runs
- Directions: LR/RL/TB/BT with goldens
- CLI: `--ascii`, `--width N` (default 100), `--strict`, `--audit=json`;
  normal rendering is invariant-checked and exits 70 without stdout on an
  internal geometry failure
- Tests: behavior + IR/render goldens + B14/B16 scene invariants + exact
  topology-aware quality contracts (`tests/quality.rs`)
- Quality audit: `cargo run -q --example symmetry` reports hard failures,
  doubled-cell relational residuals, crossings, bends, and wire length
- CLI audit: `--audit=json` emits byte-stable `llmaid.audit.v1` for every
  shipped type with normalized bounds/counts and named invariant violations;
  mindmaps report exact level counts and flowcharts add the geometry metric vector
- Generated coverage: all 71 non-empty forward DAGs on 2–4 nodes render in all
  four directions (284 cases); opposite directions have exact audit signatures
- Generated design coverage: 40 state/class/ER direction renders verify
  invariants, determinism, ASCII purity, semantics, and opposite envelopes
- Generated hierarchy coverage: all 197 ordered tree shapes through seven
  nodes plus deep, wide, mixed, Unicode, tight-width, and ASCII cases verify
  determinism, source order, label survival, and Scene invariants
- Guarantee boundary: engine-wide grid/determinism rules and scene invariants
  are distinct from topology-specific aesthetic contracts. Known chains,
  forks, merges, eligible diamonds, group boundaries, and ordered tree spans
  have exact tests; goldens do not prove arbitrary unclassified topologies beautiful. See
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

1. **Phase 4.2** — planning timelines (`gantt` or `timeline`)
2. **Phase 4.3** — `gitGraph`, then selective charts
3. Remaining agent diagnostics (JSON parser errors / optional JSON IR input)
4. Optional visual polish: native state pseudo-markers and nested
   state/subgraph handling

Track detail in `ROADMAP.md`; tick coverage in `MATRIX.md`.

### Suggested next pickup

Apply the same thin vertical-slice discipline to a time-axis engine: type-
specific ordered IR, shared `Scene`, behavior contract, deterministic generated
coverage, focused goldens, and visual review.

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
