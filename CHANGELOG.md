# Changelog

All notable changes and the decisions behind them. Newest first.
Decision entries explain *why*, so future work doesn't relitigate them.

## [Unreleased]

### Fixed
- Sequence calls and returns no longer rely on subtly different dot density or
  unusual dash glyphs: calls use a solid shaft plus filled arrowhead (`────▶`),
  while returns use a solid shaft plus thin directional arrow (`←────`). ASCII
  uses the familiar `---->` / `<----`; dotted lifelines remain separate.
- Fifth-round gallery review: equal-width flipped vertical chains choose their
  shared width parity from the terminal/top label. This makes `top` exactly
  centered with equal visible padding while both BT boxes remain identical;
  even-length labels retain only the unavoidable half-cell residual.
- Fourth-round gallery review: normalized vertical chains retain identical box
  widths and use a deterministic right bias when odd/even text parity offers
  two equally near positions around the arrow column. Root vertical forks keep
  two interior cells between attachment ports and box corners, giving `AST`
  more visual breathing room without changing its straight shafts.
- Third-round gallery review: acyclic, non-reconverging horizontal forks may
  expand across child rows. `shapes` now has zero bends after `rect`, with the
  label vertically centered in the expanded box. Approved diamond trunks and
  feedback graphs are excluded.
- Second-round gallery review: vertical edge labels now occupy the exact middle
  row of equal rank gaps. Lone vertical fork/merge boxes widen across distinct
  attachment columns, and collision-free long-edge dummy lanes remain on a
  source/target column. This removes every avoidable bend from `forkmerge`,
  `ignored-directives`, and `nested-merge` while parallel edges keep their
  separate lanes. A grouped fork with internal and external children staggers
  the external child one rank later, preserving both straight shafts and the
  external-node/frame non-intersection contract.
- Bulk-review feedback now has exact routing contracts: eligible diamond motifs
  share one centered fork/merge trunk and mirrored jog track; simple vertical
  chains use the widest standard box and one centerline; horizontal bend labels
  sit on their outgoing branch; and self-loops have a readable return leg.
  Junction sharing is intentionally limited to immediate reconverging diamonds,
  preserving already-approved unrelated forks, merges, and shape galleries.
- Alignment legalization now balances a crowded rank around its desired
  doubled-cell centers instead of pushing every collision in one direction.
  Merges prefer incoming lanes, forks prefer outgoing lanes, and the sweep ends
  downstream; this preserves symmetric fork/merge output without the unsafe
  post-layout merge snap.
- Edges may no longer intersect or ride non-endpoint boxes (B16). A nested
  merge with a long edge exposed the old post-snap collision and is now a
  permanent behavior + golden regression case.
- Goldens updated where output is clearly better: `fanout` (symmetric elbows),
  `diamond` Result, `forkmerge` VM/Value, `edge-labels`.

### Added
- Core sequence diagrams (B17): top-level diagram dispatch, declared and
  implicit participants/actors, padded headers, dotted lifelines, ordered
  `->>` messages, `-->>` dashed returns, deterministic Unicode/ASCII output,
  line-specific parser errors, and shared-scene invariant coverage. The
  `sequence-core` IR/render golden establishes the first non-flowchart type.
- Generic shared-scene paths plus scene-owned box/line paint styles. The Scene
  layer no longer imports flowchart parser types; flowchart and sequence
  engines now meet only at the geometry boundary.
- Documented the quality guarantee boundary and enforcement loop: the integer
  grid supplies exact coordinates, reusable topology constraints supply the
  aesthetic rules, goldens prevent known regressions, and human review finds
  preferences not yet formalized. The roadmap now calls for `--audit=json`
  named violations plus generated/metamorphic small-graph coverage.
- Golden review workflow (`scripts/review-gallery.py`): an all-case browser app
  with bulk pass/needs-work controls, annotations, progress, JSON import/export,
  browser-local persistence, and a local server that atomically autosaves into
  `.llmaid-review.json`. Browser diagrams are painted in explicit terminal-cell
  widths, so wide CJK and emoji glyphs cannot shift following geometry when a
  proportional fallback font is used. The same tool retains committed-vs-live
  terminal slideshows for final glyph fidelity. Covered by stdlib unit + HTTP
  tests.
- Exact topology-aware geometry audit (`audit.rs`, `tests/quality.rs`): hard
  violations; rank, mono-chain, fork, merge, and eligible-diamond residuals;
  true perpendicular crossings; bends; wire length. Centers use doubled
  integers, and parity-forced half-cell offsets are separated from avoidable
  error. The old global MAD/balance/nearest-mirror scores were removed.
- Routing/renderer boundary: signed screen-space `Scene` primitives now own
  complete paths, arrows, label positions, one-time normalization, and exact
  bounds. `render.rs` is a pure painter (down from 974 to ~600 lines); the
  duplicate render-time routers, canvas sizing, `ScreenMap`, and origin-shifted
  output margins were deleted.
- Vertical labelled edges reserve deterministic per-edge rows, fixing lost and
  overwritten labels on TB/BT parallel edges and labelled fork/merge diagrams.
- Channel bend tracks now reuse non-overlapping cross-axis intervals, tightening
  fork/merge, subgraph fan-out, and shape-gallery output without collisions.
- Title-driven subgraph growth preserves the member content's center parity, so
  single-column groups keep their frame, nodes, and exit shaft exactly aligned.
- Cluster-aware cross-axis spacing keeps external nodes disjoint from group
  frames. Grouped nodes prefer internal neighbors as alignment anchors, so an
  outside edge bends through a frame gate instead of pulling the internal spine
  toward—or placing its target inside—the group.
- B14 scene invariants now verify each edge's exact label cells, orthogonal path,
  painted endpoints, and arrow adjacency; regression coverage includes vertical
  parallel edges and labelled fork/merge.
- Layout barycenter ordering now compares exact integer ratios (no floats), and
  clippy with warnings-as-errors passes across all targets.
- Subgraph aesthetics: interior title band (not on the border stroke), larger
  pad (2), nested parent clearance so Outer/Inner titles do not collide.
- Phase 1 subgraphs: parse `subgraph`/`end` with membership + optional title;
  layout bounding-box clusters with pad/title strip; render titled rounded
  frames (B15). Nested parent stack supported; edges may cross group borders.
- Phase 0 flowchart polish: TB/BT on-shaft edge labels (beside vertical runs);
  RL/BT/TB direction goldens; mono-rank centerline straighten + mono-edge port
  snap (simple chains no longer jog); tighter vertical channels.
- Docs: `ROADMAP.md` (phased plan: flowchart polish → subgraphs → sequence →
  design types → planning → charts → agent diagnostics → distribute) and
  `MATRIX.md` (capability × tool coverage: llmaid, termiflow, termaid, diagon,
  graph-easy, mermaid.js). Product stance: agents create/self-debug, humans
  view. Linked from `AGENTS.md`, `HANDOFF.md`, `DESIGN.md`.
- Testing (B14): rendered `.txt` golden snapshots byte-compared; canvas
  invariants (closed borders, label cells intact, edge endpoints, no
  truncation glyphs) via `render::render_with_checks` on every case.
- Layout (B9/B10): `--width` overflow ladder — normal → compact gaps → wrap
  labels under pressure → over-width if still needed; never truncate or fail.
  Comfortable budgets leave single-line labels intact.
- Renderer (B13): non-rect shapes stay rect-framed with hint glyphs — ◇
  diamond corners, ( ) stadium/circle caps, ═ cylinder lid, ╱╲ hex facets;
  house style keeps rounded corners on rect/rounded (D7/D13).
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

- **D18 — Diagram-specific semantics, shared terminal geometry.** Top-level
  `diagram.rs` detects the Mermaid type and dispatches into independent IR and
  layout engines. Paint-level box and line styles live with `Scene`, and
  non-semantic paths (such as lifelines) are generic scene primitives. The
  established flowchart parser/layout API remains compatible. Rejected: force
  sequence concepts into `Graph`, or encode lifelines as fake flowchart edges;
  both would couple unrelated layout rules and weaken the D15 boundary.

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

- **D15 — Scene is the shared engine boundary.** Diagram-specific layout and
  routing emit signed screen-space rectangles, paths, arrows, and text; one
  normalization pass produces exact canvas bounds. The renderer only rasterizes
  these primitives. This keeps future sequence/state/tree engines separate at
  the semantic level while sharing terminal painting and invariants.

- **D16 — Quality is an exact vector, not a beauty scalar.** Correctness gates
  first; topology-eligible alignment and symmetry use doubled integer centers;
  crossings, bends, wire length, and area remain descriptive trade-offs.
  Global balance and nearest-neighbor mirror scores were rejected because they
  grade asymmetric graphs and can disagree with accepted visual improvements.

- **D17 — Geometry contracts, not the grid alone, define enforceable taste.**
  Engine-wide invariants cover every scene; aesthetic checks apply only when a
  graph contains their recognized topology. Goldens are regression examples,
  not a proof over arbitrary graphs. Each accepted visual preference therefore
  lands as a minimal case and an exact named predicate before implementation.
  Machine-readable per-input audit violations and generated/metamorphic graph
  coverage are the path to stronger assurance without pretending subjective
  beauty is a scalar or a theorem.
