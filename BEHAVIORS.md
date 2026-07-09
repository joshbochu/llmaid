# Behavior contracts

Every behavior here is a promise to users (mostly: coding agents piping Mermaid
through llmaid). Each has a given/when/then test named `b<N>_...` — parser and
CLI behaviors in `tests/behavior.rs` now; layout/render behaviors land with
their milestone (marked *pending*). Decisions behind these: `CHANGELOG.md` D9–D14.

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
- **B5** Given unknown directives (`classDef`, `subgraph`, …), when parsed,
  then they warn and are ignored; `--strict` upgrades warnings to failure.

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

## Layout & rendering (tests land with M2/M3)

- **B9** *pending M2* — Given a diagram wider than the width budget, when
  rendered, then degradation is: compact gaps → wrap labels → render over-width
  anyway. Never truncate a label, never fail on overflow.
- **B10** *pending M2* — Given a label that fits, when rendered, then it stays
  on one line; wrapping happens only under width pressure (B9), never at an
  arbitrary box-width cap.
- **B11** Given a self-loop (`eval --> eval`) or cycle back edge, when
  rendered, then a tight loop or perimeter route returns to the target with
  its arrow and label preserved.
- **B12** Given parallel edges (B3), when rendered, then both are drawn as
  distinct paths, each carrying its own label.
- **B13** *pending M2* — Given non-rect shapes, when rendered, then boxes are
  rect-framed with shape-hint glyphs (◇ corners, rounded caps, cylinder lid) —
  grid alignment is never risked for shape fidelity (termiflow's failure mode).
- **B14** *pending M2* — Given any rendered frame, then invariants hold:
  no truncated labels, all borders closed, every edge reaches both endpoints,
  no character overwrites label text. (Doubles as the fuzz oracle.)
