# Changelog

All notable changes and the decisions behind them. Newest first.
Decision entries explain *why*, so future work doesn't relitigate them.

## [Unreleased]

### Added
- Semantic final-render inspection (B34): `--inspect=json` emits stable,
  dependency-free `llmaid.inspect.v1` for every shipped diagram type. The
  report combines semantic element identities with normalized final-Scene
  boxes, groups, paths, edges, decorations, texts, and exact raster rows. Each
  check has an invariant/preference/budget class, explicit applicability and
  pass/fail/not-applicable status, plus semantic elements and exact integer
  witnesses on failure. Compositions without a sound predicate are listed as
  `unclassified`; no global beauty score is introduced. The existing
  `llmaid.audit.v1` schema and clean-report bytes remain separate and unchanged.
- An independent cross-engine quality evaluator over the final Scene, with
  endpoint/containment/structure checks and topology-specific preferences for
  flowcharts, sequences, state/class/ER diagrams, mindmaps, and timelines.
  Mutation tests prove it detects shifted or damaged final geometry without
  consulting layout intermediates. The 35-case reviewed gallery gates every
  applicable invariant and preference, while all generated corpora now gate
  semantic invariants across their broader input spaces.

### Changed
- Runtime invariant failures now direct agents to the richer
  `--inspect=json` report. `--audit=json` and `--inspect=json` are explicitly
  mutually exclusive machine-output modes.

### Fixed
- Sequence message labels now center their occupied cell extent between the
  actual attachment cells instead of subtracting the full width and biasing
  labels one column left. Four sequence goldens record the improved centering.
- The timeline connector-padding predicate counts cells strictly between text
  and attachment, avoiding an asymmetric false failure for leading labels.

### Documentation
- The README now documents crates.io installation, upgrade/reinstallation,
  removal, and the Cargo binary location.
- The inspection guide now explains the verification architecture for humans
  as well as agents: its parser-to-Scene-to-evaluator data flow, quantitative
  contracts, decision policy, worked examples, trust boundaries, and the roles
  of mutation tests and gallery review. `AGENTS.md` carries the condensed
  operating policy so future coding-agent sessions inherit it immediately.
- The design now states the semantic quality guarantee model and why raw grid
  coordinates are evidence rather than a fixed-placement oracle; agent,
  roadmap, handoff, behavior, and capability docs cover the inspection loop.

### Decisions

- **D34 — Inspect semantic relations on the final Scene; do not grade a fixed
  placement grid.** Diagram IR supplies stable semantic identity and intent,
  while the normalized final Scene and checked raster supply independently
  measured evidence. Exact predicates describe relationships that matter—such
  as endpoint attachment, containment, chronology, centering, symmetry,
  padding, and structured compartments—without requiring every valid render to
  occupy one absolute coordinate template. `llmaid.inspect.v1` therefore
  exposes both raw grid/canvas data and typed checks with applicability,
  semantic witnesses, and explicit unclassified compositions. Reviewed
  goldens gate invariants plus preferences; broad generated corpora gate
  invariants; mutation tests verify evaluator independence. The richer schema
  is separate from byte-compatible `llmaid.audit.v1`. Rejected: fixed-grid or
  pixel snapshots as the primary oracle, self-checks over layout-owned
  intermediate values, an LLM-only visual judge, and a scalar beauty score.

## [0.1.0] - 2026-07-31

### Changed
- Flowchart rank channels now reserve rows from visible geometry instead of a
  blanket five-row minimum: one row for a straight arrow, two per reusable
  fork/merge track, and only the measured rows needed by labels and endpoint
  adornments. Distinct-peer junctions keep content-sized boxes and share a
  centered external track; feedback and labeled horizontal merges retain
  separate ports where their topology requires them. Group boundaries keep
  cumulative frame padding plus one visible separation cell. Representative
  vertical goldens shrink from 35→21 rows (`forkmerge`), 19→13
  (`nested-merge`), 23→18 (`ignored-directives`), and 19→13
  (`dir-tb-labels`) while all exact eligible alignment residuals reach zero.
- Sequence `alt` / `else` now renders as one containing `alt` frame with a
  labeled full-width branch separator, instead of nesting a second frame for
  `else`. Controls inside the alternate branch retain one clear inset, and the
  separator joins both frame sides and every lifeline in Unicode and ASCII.
- Width fallback now optimizes for readable output rather than forcing every
  rank toward one tiny cap. Flowchart, mindmap, timeline, and sequence labels
  wrap only at word boundaries with an eight-column floor; identifiers stay
  intact and accept honest overflow. Sequence diagrams now compact and wrap
  under `--width`, while class/ER tables preserve their columns and compact
  only relationship channels. The default structured-chain goldens shrink from
  128→116 columns (`class-relations`), 115→109 (`er-basic`), and 112→100
  (`er-cardinalities`).
- `--ascii` now means ASCII structural glyphs rather than rewriting user label
  text. Solid, dotted, and thick straight segments retain distinct ASCII
  styles, and the help text states the boundary explicitly.
- The sequence engine is now organized as `sequence/{ir,parse,layout,dump}.rs`
  behind the unchanged `sequence::parse`, `sequence::scene`, and
  `sequence::dump` API. This separates semantic data, Mermaid syntax,
  geometry, and debugging responsibilities without changing rendered bytes or
  behavior contracts.
- Roadmap breadth is deliberately paused after the shipped flowchart,
  sequence, state/class/ER, mindmap, and timeline core. `gitGraph` remains
  available as a tested parked branch (`codex/git-todo-later` at `7f2989b`),
  while charts and boards move to explicit non-goals until concrete demand
  outweighs their terminal visual-grammar and maintenance cost. Active work
  now favors gallery-driven polish, machine-readable errors, and distribution.
- The Python golden-review server remains the sole browser review workflow and
  adopts the card carousel and keyboard UX prototyped in React. It autosaves one
  case at a time and persists only annotated cases to `.llmaid-review.json`.

### Fixed
- CI fixtures now retain canonical LF endings on Windows, preserving exact
  golden byte comparisons across platforms. CLI subprocess tests also accept
  the legitimate broken pipe produced when argument validation exits before
  consuming irrelevant stdin.
- Group frames now merge their own stroke directions with connectors crossing
  any side, producing a real tee/crossing instead of a vertical or horizontal
  hole that merely occupied the same cell. Border invariants require the exact
  side/corner orientation while allowing legitimate extra crossing bits.
  Flowchart group titles choose the nearest deterministic clear interior span,
  so vertical entry/exit paths no longer overwrite centered titles.
- Terminal text is painted as measured extended grapheme clusters, so
  combining accents and emoji ZWJ sequences no longer consume a fake extra
  cell or erase a following border. Explicit line breaks now own separate
  `SceneText` rows (including flowchart edge labels), while C0/C1 controls,
  tabs, and bare carriage returns fail with a source line before normal render
  or audit. Checked rendering rejects unsafe programmatic scene text, the
  painter never forwards a control scalar, legitimate user ellipses are no
  longer mistaken for truncation, parsed standalone zero-column graphemes fail
  without misclassifying combining marks attached to visible punctuation, and
  border checks cover complete perimeters rather than four corners.
- Known unsupported Mermaid document headers now fail directly instead of
  falling through as plausible headerless flowcharts. Parse diagnostics name
  the file or stdin plus the offending source line; `--width 0` and multiple
  input sources fail during argument parsing; and a downstream closed pipe is
  handled as successful pipeline termination instead of a Rust stdout panic.
- Golden reviewer: mixed-width CJK and emoji use the original Python terminal
  cell painter, avoiding browser fallback-font advances without breaking joined
  sequence lifelines and frame strokes.
- Phase 4.2 gallery correction: diagram titles now center on the existing
  compact chronological spine. Connector strokes reserve one more trailing
  cell than leading gap because the leading endpoint is inclusive; their odd
  cell span now has the spine at its exact midpoint. An even-width title keeps
  only its unavoidable half-cell residual. A rejected intermediate attempt
  balanced the entire unequal label envelope by stretching the shorter lane;
  screenshot review exposed the resulting long wires, so exact contracts now
  protect compact, midpoint-correct gaps instead.
- Phase 3.4 gallery feedback: class/ER relationship channels reserve explicit
  endpoint space; diamonds, triangles, and arrows sit two cells from box
  borders; ER min/max marks have a line cell between them; and dependency
  arrows use a stronger filled head. Multiplicity labels move with the widened
  endpoint footprint instead of crowding class borders.
- Generated small-graph coverage found a fork box widening across an unrelated
  long-edge dummy lane. The late quality pass now refuses that expansion while
  retaining incident attachment lanes; the minimized four-node case is a
  permanent regression.
- Sequence calls and returns no longer rely on subtly different dot density or
  unusual dash glyphs: calls use a solid shaft plus filled arrowhead (`────▶`),
  while returns use a solid shaft plus thin directional arrow (`←────`). ASCII
  uses the familiar `---->` / `<----`; dotted lifelines remain separate.
- Fifth-round gallery review: equal-width flipped vertical chains choose their
  shared width parity from the terminal/top label. This makes `top` exactly
  centered with equal visible padding while both BT boxes remain identical;
  even-length labels retain only the unavoidable half-cell residual.
- Earlier gallery passes established the deterministic vertical-chain parity
  bias and exact middle-row placement for vertical edge labels. D30 supersedes
  their routing-driven fork/merge box expansion with natural boxes and shared
  external tracks while retaining exact eligible alignment.
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
- A concise product README, explicit Rust 1.88 package metadata, and a
  cross-platform CI quality gate covering the MSRV, formatting, Clippy, Python
  gallery tests, and the full Rust suite on Linux, macOS, and Windows.
  `cargo package` contents and the live README example are verified. The project
  is MIT licensed, and crates.io is the first release channel.
- Named audit quality and fit diagnostics (B33): nonzero exact flowchart
  alignment, symmetry, and crossing residuals plus over-width fallback now
  appear in the existing `llmaid.audit.v1` violation vector with deterministic
  names and structured metric/width witnesses. The public audit API exposes
  the same ordered diagnostic vector; descriptive bend and wire totals remain
  metrics rather than an invented scalar score.
- Terminal-safe text behavior (B32), with cross-engine grapheme/control
  coverage, direct canvas regressions, and a `terminal-text` golden covering
  combining marks, a ZWJ emoji, multiline edge text, and a legitimate ellipsis.
- `unicode-segmentation` as the Unicode grapheme-boundary counterpart to
  `unicode-width`; see D28.
- Phase 4.2 core timelines (B26): `timeline` dispatch, a type-specific ordered
  title/period/event/section IR, inline and continuation events, strict
  line-specific placement/syntax errors, a reusable semantic-free integer
  temporal layout, compact common-spine Scene geometry, deterministic
  Unicode/ASCII output, B9 width fallback, typed audit JSON with semantic
  period/event counts and chronological ranks, 170 exhaustive small structures,
  deep/broad/Unicode/long-label stress coverage, exact temporal geometry
  contracts, and five focused goldens. Calendar arithmetic, gantt bars,
  direction variants, and styling remain explicitly deferred; D28 now supplies
  the shared extended-grapheme behavior.
- Phase 4.1 core mindmaps (B25): `mindmap` dispatch, a type-specific ordered
  indentation IR, strict line-specific errors, a reusable native integer tree
  layout, arrowless shared-trunk Scene geometry, deterministic Unicode/ASCII
  output, B9 width fallback, typed audit JSON with exact level counts, 197
  generated ordered-tree shapes, exact parent-child quality contracts, and
  balanced/deep/wide/Unicode goldens. Canonical `root((label))` is accepted as
  a label spelling; icons, styles, classes, Markdown, and general node shapes
  remain explicitly deferred. D28 now supplies shared extended-grapheme
  behavior.
- Phase 3.4 terminal visual fidelity: class headers and members now use closed
  compartments; aggregation/composition/inheritance/dependency/realization use
  endpoint diamonds, arrows, and triangles; multiplicities sit beside their
  endpoints. ER entities render aligned type/name/key/comment tables with row
  dividers, while zero/one/many glyphs attach to relationship endpoints. Raw
  UML/ER operators no longer substitute for visual notation. The primitives
  are shared, deterministic, ASCII-safe, invariant-checked, and covered by
  exact adjacency/table geometry contracts plus `class-relations` and
  `er-cardinalities` goldens.
- Phase 3 design-document diagrams (B21–B23): flat `stateDiagram` /
  `stateDiagram-v2`, core `classDiagram`, and core `erDiagram` now dispatch to
  independent semantic parsers and lower through a shared typed-box adapter.
  State aliases/markers/transitions, class members/UML relations, and ER
  attributes/keys/cardinalities remain visible in deterministic Unicode and
  ASCII output, with per-type line diagnostics, audit JSON, generated coverage,
  behavior tests, and IR/render goldens.
- Runtime invariant enforcement (B24 / Phase 6.5): the normal CLI render path
  now runs the same scene checks as tests. A failure writes exact diagnostics
  and an audit hint to stderr, keeps stdout empty, and exits 70 instead of
  publishing corrupt geometry.
- Stable machine geometry audit (B19): `--audit=json` emits
  `llmaid.audit.v1` for flowcharts and sequences with normalized bounds,
  element counts, deterministic named violations, exact witnesses where
  available, generic scene invariant failures, and the exact flowchart metric
  vector. JSON is handwritten and dependency-free; warnings stay on stderr.
- Deterministic generated/metamorphic coverage: all 71 non-empty forward DAGs
  on two through four nodes render in LR/RL/TB/BT (284 cases), rerender
  byte-identically, satisfy scene/audit invariants, and preserve exact LR↔RL
  and TB↔BT comparable signatures.
- Nested sequence controls (B20): canonical `loop`, `alt` / `else`, and `opt`
  directives are recorded at exact event boundaries, validated with
  line-specific errors, and rendered as labeled containing frames in Unicode
  and ASCII. Nested `else` branches and their child blocks have exact
  containment/inset/lifeline-span contracts plus a focused golden.
- Sequence notes and activation (B18): ordered source events; `Note left of`,
  `Note right of`, and `Note over` one/two participants; balanced and nested
  explicit `activate` / `deactivate`; line-specific errors; deterministic
  Unicode/ASCII output; and exact placement, bar-attachment, spacing, and
  self-loop contracts. Consecutive notes share a compact event cadence while
  activation caps retain a dedicated row above message labels.
  `sequence-notes` and `sequence-activation` add IR/render goldens and
  shared-scene invariant coverage.
- Generic opaque foreground boxes at the shared `Scene` boundary. Notes and
  activation bars paint over non-semantic lifelines while semantic messages
  remain in front; bounds, normalization, border/label checks, and edge-box
  collision checks include the new layer.
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
  preferences not yet formalized. `--audit=json` and generated/metamorphic
  small-graph coverage now expose and enforce that boundary programmatically.
- Golden review workflow: an all-case browser app with bulk pass/needs-work
  controls, annotations, progress, JSON import/export, browser-local
  persistence, and a local server that atomically autosaves into
  `.llmaid-review.json`.
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
  invariants (complete closed borders, exact grapheme/continuation cells, and
  edge endpoints) via `render::render_with_checks` on every case.
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

- **D33 — A frame crossing is a bitmask union, not a punched hole.** Group
  borders merge their required line directions into existing connector cells,
  letting the shared glyph table resolve a continuous tee or crossing. The
  invariant gate checks the required mask for every side and corner while
  permitting additional connector directions. Flowchart routing places a
  title in the nearest unoccupied interior span after edge geometry is known;
  the painter never chooses between destroying text and destroying a path.
  Rejected: painting frames only into empty cells, accepting any line
  orientation as a closed border, and hiding a connector underneath a title.

- **D32 — Alternate branches subdivide one semantic frame.** Sequence layout
  lowers `else` to a labeled horizontal separator inside its containing `alt`
  `SceneGroup`; nested controls keep their ordinary control depth rather than
  gaining a fake branch-frame depth. `SceneGroupSeparator` is a narrow generic
  scene primitive whose complete stroke, label, frame joins, and lifeline
  crossings are painted and checked without exposing sequence syntax to the
  renderer. Rejected: the previous nested `else` frame (excess ink and false
  hierarchy), type-specific painter logic, and an unframed floating branch
  label with no exact geometry contract.

- **D31 — Audit v1 already has the right diagnostic envelope.** Exact nonzero
  topology residuals and width-target overflow are serialized as new stable
  names inside the existing `violations` vector, whose per-name witness was
  already an open structured value. This preserves the byte shape and output of
  clean v1 reports without a schema bump or a parallel advisory field. Metric
  witnesses carry the exact doubled-cell value; fit witnesses carry target,
  rendered width, and overflow columns. These entries make imperfection
  inspectable but do not make permitted B9 overflow a render failure. Bends and
  wire length stay descriptive until a topology-specific contract proves one
  avoidable. Rejected: `llmaid.audit.v2` for an additive use of the existing
  envelope, opaque prose-only diagnostics, and any global beauty/severity
  score.

- **D30 — Flowchart whitespace follows visible geometry; junctions branch
  outside natural boxes.** Vertical rank channels reserve one straight-arrow
  row, two rows per reusable bend track, measured label rows plus the terminal
  arrow row, and explicit endpoint/group clearance. Acyclic distinct-peer
  forks and merges attach at one centered box port and share the lowest
  external track instead of inflating the node across every peer lane. A
  bounded legal-slot pass may align a mono endpoint without reordering
  siblings or swallowing long-edge lanes; horizontal rank members center on
  their rank extent. Exact fork/merge barycenter diagnostics apply only to
  feedback-free junctions, matching the router's eligibility rule. Rejected:
  the blanket five-row vertical gap, extreme junction widening solely to
  achieve zero bends, grading perimeter-feedback junctions as avoidably
  off-center, and compaction that lets external boxes touch group frames.

- **D29 — Width is a readability target, not permission to make text
  vertical.** The B9 ladder evaluates rendered width after normal and compact
  spacing, wraps only complete words with an eight-column floor, and chooses
  the largest cap that fits. Whitespace-free developer tokens remain whole; if
  readable wrapping cannot meet the target, the result may stay over-width and
  wrapping is retained only when it reduces the overflow. Sequence headers,
  messages, and notes participate in the same ladder. Structured class/ER
  tables use compact unwrapped geometry because wrapping placeholder rows would
  corrupt their final columns. Rejected: one-character columns, slicing
  identifiers, blindly returning an over-wrapped candidate, and treating
  `--width` as a destructive hard maximum.

- **D28 — Extended grapheme clusters are the terminal paint atom.** Canvas
  text cells retain one complete Unicode grapheme plus its measured
  continuation cells, using `unicode-segmentation` for boundaries and
  `unicode-width` for columns. This preserves combining marks and emoji ZWJ
  sequences without reimplementing Unicode tables or broadly rejecting valid
  labels. Parsed label boundaries, rather than raw Mermaid punctuation, decide
  whether a grapheme owns a visible cell. `SceneText` represents explicit
  newlines as measured rows; control scalars never reach a cell and are
  rejected with source-line diagnostics, with checked rendering as the
  programmatic safety backstop. Truncation is verified by exact grapheme/run
  continuity and full border closure, not by banning ellipsis glyphs that
  users may intentionally write. Rejected:
  treating every scalar as at least one cell, silently stripping valid
  zero-width joiners, and maintaining a hand-written segmentation table.

- **D27 — CLI failures are source-aware and pipeline-native.** Type dispatch
  rejects a conservative catalog of exact unsupported Mermaid headers before
  the forgiving headerless-flowchart fallback, while actual flowchart
  statements remain legal without a header. Human parse errors use
  `source:line`, a repairable expectation, and a terminal-safe source excerpt.
  stdout is written explicitly through a buffered writer; `BrokenPipe` means a
  downstream consumer stopped normally, while other write failures use exit
  74. Exactly one FILE/`-` source and a positive width are validated before
  reading. ASCII mode transforms structure, not arbitrary label text, and
  preserves edge-kind distinctions. A new machine-error flag remains deferred
  until its schema warrants expanding the intentionally tiny CLI.

- **D26 — Timeline semantics own a reusable plain temporal spine.** Phase 4.2
  parses free-text periods, source-ordered events, and contiguous named section
  ranges into an independent IR, then lowers measured slots through a
  string-free integer temporal engine. Period labels right-align left of one
  fixed vertical spine; event branches attach on exact source-ordered rows;
  named sections are separated containing frames; and every connector retains
  a visible blank cell before text. Width fallback stays in `timeline.rs`, and
  audit ranks mean chronological periods. A local termaid 0.7.1 comparison
  supported the compact rail but exposed ambiguous continuation ownership,
  non-containing sections, and loose over-width behavior. Rejected: fake
  flowchart ranks, boxed event cards (too tall for a changelog scan), invoking
  Mermaid.js/termaid, and premature date/calendar or gantt-bar arithmetic.

- **D25 — Mindmaps own ordered tree geometry, not layered-graph lowering.**
  Phase 4.1 parses a strict two-space indentation subset into a flat preorder
  IR with explicit parent indices, then lowers through a semantic-free native
  integer tree engine. Source sibling order is authoritative; parents center
  exactly on their child span; arrowless shared trunks follow Unix/Diagon tree
  convention; width fallback wraps only under pressure and may render
  over-width rather than truncate. Advanced Mermaid mindmap syntax remains
  deferred; D28 now defines shared rendering for extended grapheme clusters.
  Rejected: feeding fake flowchart edges to Sugiyama (order and tidy-tree
  contracts become incidental) and embedding/invoking Diagon.

- **D24 — Relationship notation is endpoint geometry, not edge-label text.**
  Class/ER engines lower semantic adornments and structured table content into
  generic `Scene` primitives. The renderer orients glyphs for LR/RL/TB/BT and
  ASCII, while labels contain only human relationship text. Structured boxes
  preserve columns by taking B9's allowed over-width fallback rather than
  word-wrapping a table into corrupt geometry. Rejected: raw Mermaid operators
  centered on edges and delegating design-doc rendering to a lossy subprocess.

- **D23 — Invariant checks gate normal CLI output.** The runtime uses the same
  checked scene painter as tests. If internal geometry violates a contract,
  stdout stays empty, exact failures go to stderr, and exit 70 distinguishes an
  internal renderer failure from malformed input (64). Rejected: knowingly
  printing a damaged diagram and test-only invariants.

- **D22 — Design-doc types share geometry, not semantic IR.** State, class,
  and ER own separate ordered parsers and semantic models, then lower structured
  nodes and relations through a small typed-box adapter into the established
  integer layout/router. Relation operators/cardinalities lower into generic
  endpoint decorations (D24). Rejected: fake Mermaid
  flowchart source (lossy diagnostics) and one union IR coupling unrelated
  language rules.

- **D21 — Sequence controls are boundary directives, not fake events.**
  `loop`, `alt` / `else`, `opt`, and `end` attach to stable indices in the
  ordered event stream, preserving message/note/activation semantics and
  arbitrary nesting. Layout emits generic nested `SceneGroup` frames spanning
  every lifeline. Rejected: synthetic messages (wrong ordering/geometry) and
  type-specific renderer logic.

- **D20 — Audit JSON is a versioned dependency-free CLI artifact.**
  `--audit=json` replaces diagram stdout with compact `llmaid.audit.v1`; all
  diagnostics remain stderr. Flowcharts expose the exact metric vector while
  sequences use the same bounds/count/violation envelope with `metrics:null`.
  Stable machine names and normalized witnesses are separate from human error
  messages. Rejected: serde solely for one fixed schema and mixing JSON with
  rendered diagram bytes.

- **D19 — Sequence source order is semantic; foreground layering is generic.**
  Sequence messages, notes, and activation commands live in one ordered event
  stream, so layout cannot reorder interactions by maintaining parallel lists.
  Notes and activation bars emit opaque foreground boxes into `Scene`; the
  renderer knows paint order but no sequence semantics. Rejected: separate
  message/note/activation collections (order drift), paint-only activation
  glyphs (no exact geometry), or encoding sequence events as flowchart nodes.

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

- **D4 — Unicode width tables are a dependency.** Terminal column measurement is a
  maintained-Unicode-tables problem (CJK, emoji ZWJ, combining marks; shifts
  with each Unicode release) — exactly what to outsource. Canonical unicode-rs
  crate used by ratatui et al. Rolling our own rejected as re-deriving the same
  tables, worse. Known limit (all tools share it): chat UIs / markdown viewers
  with font fallback may visually wobble non-ASCII labels; real terminals align.

- **D5 — No CLI framework; std-only arg parsing.** Six flags (`--ascii`,
  `--width`, `--strict`, `--audit=json`, `--help`, `--version`) stay small by hand. clap
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
