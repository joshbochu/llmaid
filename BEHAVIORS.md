# Behavior contracts

Every behavior here is a promise to users (mostly: coding agents piping Mermaid
through llmaid). Each has a given/when/then test named `b<N>_...` — parser and
CLI and cross-engine behaviors live in `tests/behavior.rs`; engine-specific
structural coverage lives beside its engine tests. Decisions behind these:
`CHANGELOG.md` D9–D40.

## Parsing

- **B1** Given a label containing `<br/>` (or `<br>`, `<br />`), when parsed,
  then it becomes a line break in the label — agents use this constantly.
- **B2** Given a node declared twice with different label/shape
  (`A[First] … A[Second]`), when parsed, then the last declaration wins and a
  warning names the replacement. Bare references (`A` after `A[First]`) never warn.
- **B3** Given two edges between the same pair (`A -->|x| B`, `A -->|y| B`),
  when parsed, then both edges are kept — no merging, no label loss.
- **B4** Given malformed input, when parsing fails, then the error names the
  line and what was expected (so an agent can self-correct in one retry).
- **B5** Given unknown directives (`classDef`, `style`, …), when parsed,
  then they warn and are ignored; `--strict` upgrades warnings to failure.
  (`subgraph` is real layout as of Phase 1 — see B15.)
- **B15** Given a `subgraph` … `end` block, when parsed and rendered, then
  member nodes are recorded on the subgraph and a titled frame is drawn around
  them; contents are not silently flattened, and nonmember boxes never
  intersect the frame.
- **B39** Given an edge whose bare source or target ID names a declared
  subgraph, including a reference before that subgraph's declaration or to a
  nested subgraph, when parsed and rendered in any flow direction, then it
  semantically attaches to that subgraph's titled frame rather than creating a
  duplicate node box. Inspection identifies that endpoint as `group:<id>` and
  independently requires its routed attachment to lie on the group border;
  ordinary node-to-member edges keep their node identities. An endpoint named
  for an empty group fails on its edge source line with a repairable request to
  add a member node rather than panicking or emitting a phantom box.
- **B40** Given a flowchart edge with Mermaid terminal circle or cross notation
  (`--o`, `--x`, including source-side and two-ended forms) or arrowheads at
  both ends (`<-->`), when parsed and rendered in LR, RL, TB, or BT, then both
  endpoint meanings remain explicit in the semantic IR and paint as distinct,
  adjacent, direction-aware terminal marks on their own routed paths. Unicode
  uses single-cell `○`/`×` marks and ASCII uses `o`/`x`; a bidirectional edge
  has two filled directional arrowheads. Inspection exposes the terminal
  decorations/arrowhead geometry, and a final-Scene invariant independently
  requires each mark to sit one cell from its declared node or subgraph frame.
- **B37** Given a flowchart line containing semicolon-separated statements,
  quoted labels, safely-contained named or numeric character references, and
  a trailing %% comment, when parsed, then statement/comment boundaries apply
  only outside flowchart label spans (including quoted text) and entity
  references; quoted shape closers remain label text; the core XML named
  references in HTML `&name;` or Mermaid `#name;` spelling (including
  `#quot;`) plus Mermaid `#decimal;`/`#xhex;`
  numeric Unicode scalars (and deliberate HTML-style numeric compatibility)
  decode in quoted and unquoted labels, including subgraph titles; unknown or
  malformed references remain literal; and a decoded terminal control fails on
  its source line. Literal br tags retain B1 behavior, while Markdown and
  other formatting tags remain literal label text.

## CLI

- **B6** Given any input, when rendering succeeds, then stdout contains ONLY
  the diagram — warnings go strictly to stderr. Piping into a PR comment can
  never pick up diagnostics.
- **B7** Given an empty graph (no nodes: empty input, only comments, or a bare
  header), when run, then exit 0 with empty stdout and a warning on stderr —
  pipelines never break on trivia.
- **B8** Given the same input and flags, when run anywhere (any terminal, any
  TTY state), then output is byte-identical. Default width is a fixed 100;
  only `--width` changes it. No terminal detection.
- **B19** Given `--audit=json`, when any supported diagram is
  inspected, then stdout is a byte-stable `llmaid.audit.v1` JSON document
  instead of a rendered diagram. It names the diagram type, normalized bounds,
  element counts, deterministic violations, exact witnesses where available,
  and flowchart geometry metrics; diagnostics remain on stderr.
- **B27** Given a known but unsupported Mermaid document header (for example
  `gitGraph`, `gantt`, or `pie`), when parsed, then llmaid fails on that header
  line and names the unsupported type plus a supported rewrite direction; it
  never silently reinterprets that document as a headerless flowchart.
  Recognized type names used inside actual headerless flowchart statements
  remain ordinary node IDs.
- **B38** Given a first semantic line containing a supported engine type name,
  when dispatching the document, then llmaid selects that engine only when the
  whole trimmed line is exactly its header; a type name followed by flowchart
  syntax or another token remains a headerless flowchart statement.
- **B28** Given CLI input-source or width mistakes, when arguments are parsed,
  then `--width 0` and every combination of multiple FILE/`-` sources fail
  with exit 64 before input is read; one explicit `-` reads stdin normally.
- **B29** Given malformed input from a file or stdin, when parsing fails, then
  the human diagnostic names the source and line, states the repairable
  expectation, and shows that source line as an excerpt. stdout stays empty.
- **B30** Given a successful render whose downstream stdout reader closes
  early, when the buffered write receives `BrokenPipe`, then llmaid exits 0
  without a panic or diagnostic; normal non-pipeline stdout failures remain
  I/O errors.
- **B31** Given `--ascii`, when structure is rendered, then structural glyphs
  use ASCII while labels remain byte-for-byte user text. Solid, dotted, and
  thick straight segments remain visually distinct (`-|`, `.:`, and `=#`
  respectively) rather than collapsing to one line style.
- **B33** Given `--audit=json`, when a rendered width exceeds its requested
  target or an exact flowchart topology residual is nonzero, then the existing
  `llmaid.audit.v1` violation vector names each relationship in stable order
  and supplies a structured witness with the exact width or doubled-cell
  value. Descriptive bend/wire totals remain metrics rather than being
  mislabeled as avoidable failures, and no scalar quality score is introduced.
- **B34** Given `--inspect=json`, when any supported diagram is inspected,
  then stdout is a byte-stable `llmaid.inspect.v1` JSON document containing
  normalized semantic scene geometry, exact raster rows, and typed invariant,
  preference, and budget checks. Every check states its applicability and
  status; failures name semantic elements and exact witnesses; compositions
  without a sound predicate are listed as `unclassified` rather than treated
  as passes. `--audit=json` remains byte-compatible `llmaid.audit.v1`, and the
  two machine-output modes are mutually exclusive.
- **B41** Given a source, target width, semantic graph/event count, recursive
  nesting depth, or final Scene raster beyond the documented fixed bounds,
  then llmaid refuses the normal render before unbounded parsing or allocation,
  keeps stdout empty, and reports the exact observed value, limit, and a
  repair. File and stdin input stop after one byte beyond the source limit, and
  `diagram::parse` applies the same source/semantic boundary for library
  callers. Canvas dimensions and checked cell area are validated before a
  fallible allocation. `--inspect=json` stays byte-stable and valid for an
  oversized final Scene: `scene.integrity` contains the exact resource witness
  while `canvas` is empty (`width:0`, `height:0`, `rows:[]`).

## Layout & rendering

- **B9** Given a diagram wider than the width budget, when rendered, then
  degradation is: compact gaps → wrap whole words at a readable line width →
  render over-width anyway. Whitespace-free tokens such as identifiers stay
  intact, even when that makes overflow unavoidable. The selected fallback
  minimizes overflow without using narrower-than-eight-column text or wrapping
  that buys no width. Target-width overflow alone never truncates or refuses a
  render; only an independent B41 resource bound (such as raster dimensions or
  canvas cells) can refuse it.
- **B10** Given a label that fits, when rendered, then it stays on one line;
  wrapping happens only under width pressure (B9), never at an arbitrary
  box-width cap. Of the readable layouts that fit, the least-wrapped one wins.
- **B11** Given a self-loop (`eval --> eval`) or cycle back edge, when
  rendered, then a tight loop or perimeter route returns to the target with
  its arrow and label preserved.
- **B12** Given parallel edges (B3), when rendered in any direction, then each
  retains a distinct path, arrow, and collision-free label lane.
- **B13** Given non-rect shapes, when rendered, then boxes are rect-framed with
  shape-hint glyphs (◇ corners, rounded caps, cylinder lid) — grid alignment
  is never risked for shape fidelity (termiflow's failure mode).
- **B14** Given any rendered frame, then invariants hold: no truncated labels,
  every cell of every border retains its required horizontal or vertical
  stroke, every edge reaches both endpoints, and no grapheme or wide-cell
  continuation is overwritten. A connector crossing a frame merges direction
  bits so both strokes remain continuous; an unrelated line orientation never
  counts as a closed border. Enforced per scene/path/label via
  `render_scene_with_checks` + `.txt` goldens
  in `tests/golden.rs` (doubles as the fuzz oracle).
- **B16** Given any routed edge, then its interior never intersects or rides
  the border of a non-endpoint node. Enforced from exact `Scene` geometry for
  every golden; nested and long-edge merges are explicit regression cases.
- **B32** Given labels containing combining marks, emoji ZWJ sequences,
  legitimate ellipses, or `<br>` line breaks, when any shipped engine renders
  them, then extended grapheme clusters occupy their measured terminal cells,
  user-authored ellipses remain text, and each explicit line owns a separate
  geometry row. C0/C1 controls, tabs, and bare carriage returns fail on their
  source line before render or audit; CRLF remains a normal line ending.
  A parsed label made from a standalone zero-column grapheme also fails on its
  source line, while a combining mark attached to any visible base—including
  visible punctuation—remains valid.
  Programmatic `Scene` construction has the same checked-render safety
  backstop, and unchecked painting never forwards a terminal control byte.

## Sequence diagrams

- **B17** Given a core `sequenceDiagram` containing declared or implicit
  participants/actors plus `->>` messages and `-->>` returns, when rendered,
  then participant order is stable and the output contains padded headers,
  dotted lifelines, ordered labeled arrows, and returns encoded with a thin
  directional arrowhead distinct from the filled call arrowhead. Labels are
  never truncated; Unicode and `--ascii` output are deterministic; malformed
  statements name the source line and expected message syntax.
- **B18** Given a `sequenceDiagram` containing `Note left of`, `Note right
  of`, `Note over` (one participant or a two-participant span), and balanced
  explicit `activate` / `deactivate` statements, when rendered, then source
  event order is preserved, note boxes occupy the named side or span without
  colliding with messages, and activation bars cover the participant lifeline
  for exactly their balanced event range. Labels are never truncated;
  Unicode and `--ascii` output are deterministic; malformed notes, unknown
  participants, and unmatched activation statements name the source line and
  expectation.
- **B20** Given balanced, arbitrarily nested `loop`, `alt` / `else`, and `opt`
  sequence control blocks, when rendered, then source event boundaries are
  preserved and labeled closed frames visibly contain their branches and all
  participant lifelines. An `alt` / `else` pair uses one containing `alt`
  frame with a labeled, full-width horizontal branch separator; `else` never
  becomes a redundant nested frame, and controls nested in either branch keep
  a visible inset inside the shared frame. Unicode and `--ascii` output are
  deterministic and never truncate labels; malformed, unmatched,
  duplicate-`else`, and unclosed directives name the source line and
  expectation.
- **B36** Given a sequence whose final semantic content is an outermost
  control fragment, when rendered, then each participant lifeline terminates
  on that frame's bottom border without a dangling row below it. If a later
  event follows the fragment, lifelines continue through the frame normally.

## Design-document diagrams

- **B21** Given a flat `stateDiagram` or `stateDiagram-v2` containing named,
  aliased, or implicit states, `[*]` start/end markers, labeled transitions,
  and a direction, when rendered, then declaration order and every transition
  are preserved with distinct connected marker nodes. Unicode and `--ascii`
  output are deterministic and never truncate labels; unsupported composite
  states and malformed statements name the source line and expectation.
- **B22** Given a `classDiagram` containing classes, visibility-bearing
  members/methods, primary UML relation operators, multiplicities, labels, and
  a direction, when rendered, then names and members occupy separated class
  compartments, UML diamonds/arrows/triangles sit at their semantic endpoints,
  and multiplicities sit beside those endpoints instead of being embedded in
  raw operator text. Relationship channels reserve a connector cell between
  every adornment and its box. Unicode and `--ascii` output are deterministic
  and malformed declarations/relations name the source line and expectation.
- **B23** Given an `erDiagram` containing aliased entities, typed attributes,
  PK/FK/UK markers, comments, zero/one/many cardinalities, identifying or
  non-identifying relations, labels, and a direction, when rendered, then
  attributes occupy aligned table rows and columns while min/max cardinality
  glyphs sit at their relationship endpoints; raw Mermaid relation tokens are
  not used as substitute labels. Cardinality marks are separated from each
  other and from box borders by visible connector cells. Unicode and `--ascii`
  output are deterministic and malformed attributes/relationships name the
  source line and expectation.
- **B35** Given two or more ER relationships that share an entity endpoint,
  including vertical `TB` / `BT` layouts, when rendered, then every
  relationship retains a distinct terminal lane, its direction-aware
  cardinality marks lie on that lane without overlapping another relation,
  and its label remains adjacent to its own routed path.

## Runtime self-checks

- **B24** Given a scene with a renderer invariant failure, when the runtime
  checked-render path runs, then no diagram is returned as successful and each
  exact failure is available for actionable stderr diagnostics. The CLI exits
  70, keeps stdout empty, and points agents to `--inspect=json` for inspection.

## Hierarchy diagrams

- **B25** Given a core `mindmap` with one root and plain descendants indented
  in two-space levels, when rendered, then the ordered parent-child hierarchy
  and source sibling order are preserved as a deterministic left-to-right
  boxed tree with arrowless shared trunks. Unicode and `--ascii` structure are
  deterministic; labels follow B9/B10 and never truncate; `--audit=json`
  reports the mindmap type and exact level count. Malformed indentation,
  missing parents, multiple roots, deferred advanced syntax, and terminal
  controls fail with the source line and a repairable expectation instead of
  producing an ambiguous or corrupt frame. Extended grapheme clusters follow
  B32.

## Planning diagrams

- **B26** Given a core `timeline` with an optional title, ordered periods,
  one or more events per period, continuation events, and named sections,
  when rendered, then source chronology and event ownership are preserved on
  one deterministic compact vertical spine with source-ordered containing
  section frames; the optional title centers on that spine. Unicode and
  `--ascii` structure are deterministic; labels follow
  B9/B10 and never truncate; `--audit=json` reports semantic period/event
  counts and chronological period ranks. Events without a current period,
  empty periods/events/sections, malformed `:` syntax, late/duplicate titles,
  empty named sections, deferred directions, and terminal controls fail with
  the source line and a repairable expectation. Extended grapheme clusters
  follow B32.
