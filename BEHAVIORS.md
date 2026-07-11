# Behavior contracts

Every behavior here is a promise to users (mostly: coding agents piping Mermaid
through llmaid). Each has a given/when/then test named `b<N>_...` — parser and
CLI and cross-engine behaviors live in `tests/behavior.rs`; engine-specific
structural coverage lives beside its engine tests. Decisions behind these:
`CHANGELOG.md` D9–D26.

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

## Layout & rendering

- **B9** Given a diagram wider than the width budget, when rendered, then
  degradation is: compact gaps → wrap labels → render over-width anyway. Never
  truncate a label, never fail on overflow.
- **B10** Given a label that fits, when rendered, then it stays on one line;
  wrapping happens only under width pressure (B9), never at an arbitrary
  box-width cap.
- **B11** Given a self-loop (`eval --> eval`) or cycle back edge, when
  rendered, then a tight loop or perimeter route returns to the target with
  its arrow and label preserved.
- **B12** Given parallel edges (B3), when rendered in any direction, then each
  retains a distinct path, arrow, and collision-free label lane.
- **B13** Given non-rect shapes, when rendered, then boxes are rect-framed with
  shape-hint glyphs (◇ corners, rounded caps, cylinder lid) — grid alignment
  is never risked for shape fidelity (termiflow's failure mode).
- **B14** Given any rendered frame, then invariants hold: no truncated labels,
  all borders closed, every edge reaches both endpoints, no character
  overwrites label text. Enforced per scene/path/label via
  `render_scene_with_checks` + `.txt` goldens
  in `tests/golden.rs` (doubles as the fuzz oracle).
- **B16** Given any routed edge, then its interior never intersects or rides
  the border of a non-endpoint node. Enforced from exact `Scene` geometry for
  every golden; nested and long-edge merges are explicit regression cases.

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
  participant lifelines. Unicode and `--ascii` output are deterministic and
  never truncate labels; malformed, unmatched, duplicate-`else`, and unclosed
  directives name the source line and expectation.

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

## Runtime self-checks

- **B24** Given a scene with a renderer invariant failure, when the runtime
  checked-render path runs, then no diagram is returned as successful and each
  exact failure is available for actionable stderr diagnostics. The CLI exits
  70, keeps stdout empty, and points agents to `--audit=json` for inspection.

## Hierarchy diagrams

- **B25** Given a core `mindmap` with one root and plain descendants indented
  in two-space levels, when rendered, then the ordered parent-child hierarchy
  and source sibling order are preserved as a deterministic left-to-right
  boxed tree with arrowless shared trunks. Unicode and `--ascii` structure are
  deterministic; labels follow B9/B10 and never truncate; `--audit=json`
  reports the mindmap type and exact level count. Malformed indentation,
  missing parents, multiple roots, deferred advanced syntax, and terminal-
  unsafe zero-width sequences fail with the source line and a repairable
  expectation instead of producing an ambiguous or corrupt frame.

## Planning diagrams

- **B26** Given a core `timeline` with an optional title, ordered periods,
  one or more events per period, continuation events, and named sections,
  when rendered, then source chronology and event ownership are preserved on
  one deterministic centered vertical spine with source-ordered containing
  section frames. Unicode and `--ascii` structure are deterministic; labels follow
  B9/B10 and never truncate; `--audit=json` reports semantic period/event
  counts and chronological period ranks. Events without a current period,
  empty periods/events/sections, malformed `:` syntax, late/duplicate titles,
  empty named sections, deferred directions, and terminal-unsafe zero-width
  sequences fail with the source line and a repairable expectation.
