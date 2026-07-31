# llmaid — agent guide

Mermaid in, clean deterministic terminal diagrams out. A single fast Rust
binary that coding agents use to compose diagrams into their output (agents
create/self-debug; humans look at the visuals).

Read `DESIGN.md` for the v1 design, `BEHAVIORS.md` for contracts (B1–B34),
`ROADMAP.md` for phased work, `MATRIX.md` for capability coverage vs other
tools, and `INSPECTION.md` for the self-verification contract. Log decisions
in `CHANGELOG.md`. Mid-stream? `HANDOFF.md`.

## Commands

```sh
cargo run -q -- diagram.mmd      # render a file
echo "graph LR; A-->B" | cargo run -q    # render stdin
cargo run -q -- --audit=json diagram.mmd # stable machine geometry report
cargo run -q -- --inspect=json diagram.mmd # semantic geometry + quality checks + canvas
cargo test                       # golden snapshots + invariants (< 5s budget)
cargo build --release            # optimized binary at target/release/llmaid

./scripts/show-gallery.sh        # eyeball all golden cases (live render)
./scripts/show-gallery.sh --txt  # same, from committed *.txt (fast)
./scripts/review-gallery.py --serve # browser review carousel; autosaves annotations
./scripts/review-gallery.py      # terminal slideshow (glyph-fidelity check)
./scripts/review-gallery.py --live  # slideshow from current renderer
./scripts/review-gallery.py --html target/llmaid-review.html
./scripts/contact-sheet.py       # packed contact sheet (terminal shelves)
./scripts/contact-sheet.py --html -o /tmp/llmaid-gallery.html
python3 -m unittest scripts/test_review_gallery.py
cargo run -q --example symmetry  # exact geometry-quality audit (all goldens)
cargo test --test inspection     # reviewed-gallery semantic quality gate
UPDATE_GOLDEN=1 cargo test        # regen tests/cases/*.{ir,txt} after intentional changes
```

## Architecture

Flowcharts retain the five-stage pipeline; diagram dispatch and other engines
join only at the shared `Scene` boundary:

```
main.rs → diagram.rs ┬→ parse.rs → layout.rs → route.rs ───────────────┐
                     ├→ sequence/ (parse → IR → layout; dump)          │
                     ├→ state.rs / class.rs / er.rs → boxed.rs ───────┤
                     ├→ mindmap.rs → tree.rs                           │
                     └→ timeline.rs → temporal.rs ─────────────────────┴→ Scene → render.rs
                                                                        style.rs (glyphs)
```

- `diagram.rs` — top-level Mermaid type detection and engine dispatch.
- `parse.rs` — Mermaid flowchart subset → IR (`Graph`: nodes, shapes, edges,
  labels, direction). Forgiving: unknown directives warn, `--strict` upgrades.
- `layout.rs` — Sugiyama layered layout in **integer grid coordinates**:
  rank assignment (cycle-breaking via DFS feedback edges), barycenter crossing
  reduction, coordinate assignment. Cells sized via `unicode-width`.
- `route.rs` — layout geometry → signed screen-space `Scene`; complete
  orthogonal paths, arrows, and collision-free label positions. Back-edges use
  the perimeter channel.
- `sequence/` — sequence engine split into its public boundary (`mod.rs`),
  ordered semantic IR, parser, deterministic dump, and integer
  lifeline/message/fragment layout.
- `state.rs` / `class.rs` / `er.rs` — independent design-doc semantic IR;
  lower structured boxes/relations through `boxed.rs` into shared geometry.
- `mindmap.rs` — strict ordered indentation IR, width fallback, and Scene
  lowering for the core plain-label mindmap subset.
- `tree.rs` — reusable semantic-free integer layout for ordered rooted trees;
  source siblings stay ordered and parents center exactly on child spans.
- `timeline.rs` — strict ordered timeline IR, width fallback, and Scene
  lowering for titles, periods/events, continuation events, and sections.
- `temporal.rs` — reusable semantic-free integer layout for ordered temporal
  ranks on a common spine with measured leading/trailing slots and band ranges.
- `scene.rs` — shared `Point` / `Rect` / path / text primitives plus structured
  tables and endpoint decorations; normalizes the finished scene once and
  derives exact bounds.
- `quality.rs` — independent semantic checks over the final normalized Scene;
  reports invariants, preferences, budgets, applicability, exact witnesses,
  and deliberately unclassified compositions without a scalar score.
- `inspect.rs` — stable `llmaid.inspect.v1` serializer for semantic geometry,
  quality reports, and the exact raster rows; `audit.rs` remains the compact
  backward-compatible geometry summary.
- `render.rs` — pure scene painter + char canvas; box-drawing junctions resolved
  via bitmask lookup
  (`─` meets `│` ⇒ `┼`).
- `style.rs` — glyph sets only; no behavior.

## Non-negotiable invariants

1. **Determinism**: same input + flags ⇒ byte-identical output. All iteration
   orders and tie-breaks must be stable (declaration order, never HashMap order —
   use Vec/IndexMap-style patterns).
2. **Labels never truncate.** Boxes grow; long labels wrap. Never insert `…`
   as a truncation marker; a user-authored ellipsis remains legitimate text.
3. **Integer coordinates everywhere.** No floats in layout/route/render.
4. **Fast**: < 10ms typical render; `cargo test` < 5s.
5. **Agent-friendly errors**: parse errors name the line and the expectation.

## Conventions

- Dependencies: `unicode-width` and `unicode-segmentation` provide terminal
  measurement and grapheme boundaries. Adding any dependency is a logged
  decision in `CHANGELOG.md` — default answer is no.
- CLI: std-only arg parsing in `main.rs` (decision: no clap). Flag surface is
  intentionally tiny; new flags need a strong reason.
- Behavior changes: any new promised behavior gets a numbered entry in
  `BEHAVIORS.md` plus a `b<N>_given_..._then_...` test in `tests/behavior.rs`.
  When implementing a *pending* behavior (B9–B14), write its test in the same
  change and drop the pending marker.
- Testing: every feature lands with golden snapshot cases
  (`tests/cases/*.mmd` + `*.txt`, byte-compared). Update snapshots only when
  you can articulate why the new output is *better*, and note it in the commit.
  Rendered-frame invariant checks (borders closed, edges reach endpoints,
  no text overwritten, edges avoid non-endpoint boxes) must pass for all cases.
- Quality changes: add an exact topology-specific contract in `tests/quality.rs`.
  Use doubled cell centers (`2 * origin + extent - 1`); do not introduce a
  global scalar "beauty" score.
- Output style: rounded corners `╭╮╰╯`, thin lines, `▶` arrowheads, 1-space
  box padding, trailing whitespace stripped, no color in v1.
- When output quality is in question, render the reference diagrams in
  `tests/cases/` and eyeball them — aesthetics are a spec here, not a nice-to-have.
  Use `review-gallery.py --serve` for bulk browser annotations, then confirm
  suspicious cases with its terminal slideshow and `--inspect=json`. Its local `.llmaid-review.json`
  contains only flagged cases and their notes, without changing committed goldens.
