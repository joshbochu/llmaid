# llmaid — v1 design

Mermaid in, beautiful terminal diagrams out. Built for coding agents to compose
their output with; optimized for human readability.

## Thesis

Agents emit Mermaid fluently (it's everywhere in their training data), but
nothing renders it in a terminal both *correctly* and *beautifully*:

- **diagon**: bulletproof alignment, but no Mermaid input and a dated aesthetic.
- **termiflow**: the right aesthetic (rounded, spacious), but a weak layout core —
  truncated labels, floating edge labels, broken diamonds.
- **termaid (PyPI)**: good coverage, but Python startup (~200ms+) and looser layout.
- **graph-easy**: clean labeled arrows, abandoned since 2010, Perl.

llmaid = diagon's correctness + termiflow's aesthetic, behind Mermaid syntax,
in a single fast binary.

## Design principles

1. **Labels are sacred.** Never truncate. Boxes size to content; long labels wrap.
2. **Edge labels sit on the arrow**: `──scan──▶`, never floating nearby.
3. **One good default, few flags.** Rounded corners, thin lines, consistent gaps,
   minimal ink. No theme zoo.
4. **Deterministic.** Same input + flags → same bytes. Output is diffable and cacheable.
5. **Fast.** < 10ms end-to-end for typical diagrams. Agents call it reflexively.
6. **Agent-friendly errors.** Parse errors name the line and what was expected,
   so a model can self-correct in one retry. Warn-and-continue where possible.

## v1 scope

Mermaid `flowchart` / `graph`, directions `LR` `RL` `TB` `BT`:

- Node shapes: rectangle `[x]`, rounded `(x)`, stadium `([x])`, diamond `{x}`,
  circle `((x))`, cylinder `[(x)]`, hexagon `{{x}}`
- Edges: `-->` `---` `-.->`  `==>`, labels via `-->|text|` and `-- text -->`
- Fork/merge (multiple out/in edges), cycles/back-edges, self-loops
- Node declaration and reference, `&` fan-out (`A --> B & C`)
- Unicode default, `--ascii` structural fallback (labels unchanged),
  `--width N` fit target

Subgraphs are supported (Phase 1). The core sequence slice supports
participants/actors (including implicit participants), lifelines, `->>`
messages, `-->>` returns, left/right/over notes, and balanced explicit
activation bars. Balanced nested `loop`, `alt` / `else`, and `opt` control
blocks render as labeled frames; `else` is a labeled horizontal subdivision
inside its single containing `alt` frame. Phase 3 adds flat state diagrams,
core class diagrams, and core ER diagrams through separate semantic IRs and a
shared typed-box geometry adapter. Phase 4.1 adds a core `mindmap` slice with
one ordered indentation-defined hierarchy of plain labels. Phase 4.2 adds a
core `timeline` slice with an optional title, ordered periods and continuation
events, and named containing sections on one vertical chronological spine.
Icons, custom classes,
Markdown, general mindmap shapes, and styling directives remain expansion work
tracked in `ROADMAP.md`; coverage lives in `MATRIX.md`.

## Architecture

```
src/
  diagram.rs   top-level diagram type detection and engine dispatch
  main.rs      CLI: args, stdin/file, error reporting
  parse.rs     Mermaid flowchart subset → IR (Graph: nodes, edges, direction)
  layout.rs    IR → integer flow-grid positions
  route.rs     Layout → complete screen-space paths, arrows, and labels
  sequence/     sequence engine with a narrow public boundary
    mod.rs      exports the stable parse / scene / dump API
    ir.rs       ordered participant, event, activation, and control IR
    parse.rs    Mermaid sequence syntax → semantic IR
    layout.rs   integer lifeline/message/fragment geometry → Scene
    dump.rs     deterministic textual IR dump
  state.rs     flat state semantic IR → boxed geometry → Scene
  class.rs     class/member/relation semantic IR → boxed geometry → Scene
  er.rs        entity/attribute/cardinality semantic IR → boxed geometry → Scene
  boxed.rs     semantic-free typed boxes/relations → layered layout + routing
  mindmap.rs   ordered indentation IR + width fallback → tree geometry → Scene
  tree.rs      reusable deterministic integer layout for ordered rooted trees
  timeline.rs  ordered period/event/section IR + width fallback → Scene
  temporal.rs  reusable deterministic integer layout for temporal ranks/bands
  scene.rs     Signed Point/Rect/Path/Text primitives + exact bounds
  render.rs    Scene → grid canvas → styled box-drawing output
  quality.rs   semantic IR + final normalized Scene → typed relational checks
  inspect.rs   semantic geometry + checks + raster → llmaid.inspect.v1
  audit.rs     compact backward-compatible geometry → llmaid.audit.v1
  style.rs     Charsets: unicode (default) / ascii
```

### Parser

Hand-rolled, line-oriented, forgiving. Unknown directives are warnings, not
errors (`--strict` upgrades them). Errors carry line numbers and expectations.

### Layout — grid-native layered (Sugiyama), integer coordinates throughout

1. **Rank assignment**: longest-path, then pull-up compaction (cycles broken by
   reversing feedback edges found via DFS).
2. **Crossing reduction**: barycenter sweeps over adjacent ranks until stable
   (bounded iterations, deterministic tie-breaks by declaration order).
3. **Coordinates**: ranks become rows (TB) or columns (LR); cells sized by
   measured label width (`unicode-width`), centered on parents' barycenter,
   spacing consistent (min gap 2 cols / 1 row; edge-label length stretches the gap).

We deliberately do *not* use the float-based `dagre`/`rust-sugiyama` crates:
character grids want integers, and snapping floats is where alignment bugs
breed. The algorithms are classic; the value is in grid-native execution.
(If layout quality stalls, `dagre` crate rank/order phases are the fallback —
keep the layout API narrow enough to swap.)

### Edge routing

Orthogonal (H/V segments only), on a routing grid between node cells:

- Straight when aligned; one elbow (`╮╰` style corners) when not.
- Labels centered on the longest straight segment of their edge; the layout
  reserves the space up front so labels never collide or float.
- Back-edges route around the diagram's edge channel.
- Arrowheads: `▶ ▼ ◀ ▲` (unicode), `> v < ^` (ascii).

### Tree layout — ordered, integer, hierarchy-native

Mindmaps do not pass through the layered digraph engine. `tree.rs` accepts
declaration-ordered parent indices and measured box sizes, assigns one depth
column per level, places leaves on an even integer stride, and centers every
parent exactly on the span from its first child to its last. Mindmap lowering
routes arrowless solid edges through a shared trunk between adjacent columns,
matching Unix/Diagon ancestry conventions while keeping llmaid's rounded boxes.

The Phase 4.1 parser deliberately uses a strict agent-fixable subset: the root
is indented two spaces under `mindmap`, descendants add exactly two spaces per
level, and labels are plain text. Canonical `root((label))` is accepted as a
root-label spelling but does not request a special shape. Advanced syntax is
rejected explicitly. Zero-width Unicode sequences are also rejected in this
slice because the current cell painter cannot preserve them without corrupting
geometry; precomposed text, CJK, and single-scalar emoji remain supported.

### Temporal layout — ordered, integer, planning-native

Timelines do not lower through flowchart ranks. `temporal.rs` accepts only
measured leading/trailing extents, declaration-ordered band ranges, and integer
spacing. It right-aligns period slots, fixes one common vertical spine, centers
each period anchor on its ordered event span, attaches every event on its exact
text row, and returns separated containing band rectangles. Compact connector
strokes use an odd cell span with the spine at the exact midpoint; the trailing
gap is one cell larger because the leading border endpoint is inclusive.
Diagram titles use that same centerline, with only the unavoidable half-cell
residual for even-width text. The layout does not stretch connectors to balance
unequal label widths. It performs no date parsing, duration calculation, or
calendar arithmetic.

`timeline.rs` owns Mermaid semantics and the B9 width ladder: unwrapped normal
spacing, unwrapped compact spacing, stable two-column whole-word wrapping, then
natural over-width output for intrinsically wide titles, sections, or labels.
Timeline labels remain plain terminal text with one visible blank cell between
text and connector; named sections reuse generic rounded `SceneGroup` frames.
This keeps the familiar compact changelog rail while making period/event
ownership and section containment exact.

### Renderer

A diagram engine emits a signed screen-space `Scene`, which is normalized once
to `(0, 0)` and measured from its exact bounds. The renderer only paints scene
primitives onto a `Canvas` of cells; it owns no routing policy.

Text cells store complete extended grapheme clusters plus their measured
continuation cells. `unicode-segmentation` supplies cluster boundaries and
`unicode-width` supplies terminal columns, so combining accents and emoji ZWJ
sequences are never approximated as one cell per scalar. `SceneText` newlines
are explicit geometry rows; control scalars are rejected before parsing can
copy them into output and are omitted by the painter as a final safety backstop.

Structured `SceneTable` content supplies class compartments and ER attribute
grids. Paint-level endpoint decorations carry UML diamonds/arrows/triangles and
ER min/max cardinalities at exact cells. Design relationships reserve their
paint footprint in layout: each adornment sits two cells from its box, leaving
one visible connector cell, and paired ER marks retain a connector cell between
them. Converging ER relationships retain declaration-ordered terminal ports so
their cardinalities never collapse onto a shared trunk; vertical relationship
labels sit beside the target-side shaft that actually crosses their reserved
label row. Type-specific engines choose the semantics; the renderer only
orients and paints the glyphs.

`SceneGroup` frames may carry labeled horizontal separators. Sequence layout
uses that generic primitive for `alt` / `else` branches, keeping control
semantics out of the painter while making the entire divider stroke, label,
and frame intersection available to shared invariant checks.

Box-drawing junction resolution (e.g. `─` meeting `│` becomes `┼`) uses a
bitmask union, so crossings and tees always retain both strokes. Frame checks
verify each border cell's required side/corner directions rather than accepting
an unrelated line merely because it occupies the same cell. Group titles are
placed after edge geometry chooses its cells, using the nearest deterministic
clear span inside the frame.

## Aesthetic spec (the defaults)

```
╭─────────╮  scan   ╭──────────────╮  parse   ╭──────────╮
│ source  ├────────▶│  Vec<Token>  ├─────────▶│ Expr AST │
╰─────────╯         ╰──────────────╯          ╰──────────╯
```

- Rounded corners `╭╮╰╯`, thin lines `─│`, filled arrowheads `▶`
- 1 space horizontal padding inside boxes; diamond/circle rendered as their
  shape hints (`◇`-cornered box, `( )` caps) without breaking alignment
- Trailing whitespace stripped; no color in v1 (pipes clean everywhere)

## CLI

```
llmaid [FILE|-]            read Mermaid from FILE or stdin, write to stdout
  --ascii                  ASCII structural glyphs; labels stay unchanged
  --width <N>              target output width (default: fixed 100 — never
                           terminal-detected, so output is byte-deterministic)
  --strict                 warnings become errors
  --audit=json             stable machine geometry report instead of a diagram
  --inspect=json           semantic geometry, checks, and raster rows as JSON
  --version / --help
```

Exit codes: 0 ok (including a downstream closed pipe), 64 usage/parse error,
70 internal render-invariant failure, 74 stdout I/O error.

Width overflow ladder (never truncate, never fail): compact inter-node gaps →
wrap whole words → render over-width anyway. Wrapping has an eight-column
readability floor; whitespace-free tokens remain intact, and the largest
wrapping cap that fits wins. If no readable wrapped result fits, it is used only
when it reduces actual overflow; otherwise the compact unwrapped result wins.
Sequence headers, messages, and notes use the same ladder. Structured class/ER
tables keep their columns but still compact relationship channels. Labels wrap
only under width pressure.
Empty graphs exit 0 (empty stdout, stderr warning; either machine mode emits a
zero report). stdout carries only the selected artifact—diagram, audit JSON,
or inspection JSON—and all diagnostics go to stderr. Parse diagnostics use `source:line`, state the
repairable expectation, and include the offending source line. Known
unsupported Mermaid document headers fail directly instead of being
reinterpreted as headerless flowcharts.

## Testing

- **Behavior contracts**: `BEHAVIORS.md` (B1–B34) indexes the promised
  behaviors; each has a given/when/then test in `tests/behavior.rs`
  (CLI contracts exercise the real binary).
- **Golden snapshots**: `tests/cases/*.mmd` → `tests/cases/*.txt`, byte-compared.
  Seed set: LR pipeline with edge labels, fork/merge, diamond decision,
  cycle/back-edge, self-loop, CJK + emoji labels, every shape, TB deep chain.
- **Invariant checks** on every rendered frame (also run as fuzz oracle):
  exact grapheme and wide-continuation cells, every border cell closed, every
  edge reaching its endpoints, and no edge crossing a non-endpoint box.
- **Geometry quality contracts** use doubled integer centers to measure exact
  rank, chain, fork, merge, and eligible-diamond relationships without parity
  loss. Hard violations and individual residuals remain a vector; aesthetics
  are never hidden behind one scalar score.
- **Generated coverage** exhausts all 71 non-empty forward DAGs on two through
  four nodes in LR/RL/TB/BT (284 renders), reruns the integer pipeline for
  determinism, and compares exact LR↔RL and TB↔BT audit signatures.
- **Generated mindmap coverage** exhausts all 197 ordered tree shapes through
  seven nodes, then adds deep, wide, mixed, CJK/emoji, tight-width, and ASCII
  cases. Exact parent-span centering, border-center attachment, and interior
  padding contracts live in `tests/quality.rs`.
- **Generated timeline coverage** exhausts 170 small period/event/section-cut
  structures, then stresses deep/broad, Unicode, long-label, tight-width, and
  ASCII cases. Exact chronological anchor, spine, attachment, section,
  padding, and collision contracts live in `tests/quality.rs`.
- **Machine audit** (`--audit=json`) exposes normalized named violations and
  witnesses plus the existing exact flowchart metric vector through a stable,
  dependency-free `llmaid.audit.v1` schema. Nonzero topology residuals receive
  deterministic metric witnesses, and every engine reports an exact
  `width_target_exceeded` witness when B9's permitted over-width fallback is
  used. Sequence scenes share the generic bounds/count/invariant envelope;
  timelines report semantic period+event nodes, event counts, and chronological
  period ranks.
- **Semantic inspection** (`--inspect=json`) evaluates the final normalized
  `Scene`, independently of each layout engine's intermediate coordinates. Its
  stable `llmaid.inspect.v1` document exposes semantic element identities,
  boxes, groups, paths, edges, endpoint decorations, texts, exact raster rows,
  and typed checks with applicability, status, per-element witnesses, and
  deliberately unclassified compositions. `tests/inspection.rs` requires all
  applicable invariants and preferences to pass over the reviewed gallery;
  generated corpora apply the invariant subset broadly, and mutation tests
  prove the independent evaluator detects damaged final geometry.
- **Junction routing** keeps content-sized boxes and branches on a shared
  external track for eligible distinct-peer forks and merges. Vertical rank
  channels reserve only the rows visible geometry needs; horizontal and
  vertical junctions use one centered box port without stretching the box
  across peer lanes. Long-edge dummy lanes may align to a source or target only
  when every intermediate box remains clear. Parallel and feedback edges still
  retain separate lanes.
- **Group clearance** is cumulative across nested frames. External children
  start beyond the containing frame with a visible separation cell; crossing
  paths merge their line bits into the frame instead of erasing its stroke.
  Symmetric diamonds retain their shared trunk, and feedback graphs retain
  perimeter routing. Labels remain centered in their natural boxes.
- **Cell parity** is explicit: equal-width boxes with odd/even label lengths
  cannot all share an exact half-cell center. Normalized vertical chains keep
  equal box widths and choose the right of the two equally near text positions,
  biased toward the shared integer arrow column. The residual remains one
  doubled unit and is classified as unavoidable by the audit.
  For flipped chains, the terminal/top label selects the shared box-width
  parity, ensuring the visually dominant destination label has equal padding.
- **Human visual review** uses `scripts/review-gallery.py --serve`: a browser carousel shows
  one committed golden per card. Arrow keys move between cards, Enter marks the
  current case OK, and Space marks it as needing notes. Dev-server saves update
  one case at a time, while `.llmaid-review.json` remains agent-facing
  annotation state: only cases with notes are persisted there. The Python
  renderer groups combining characters and locks each glyph to its terminal
  display width, so CJK and emoji font fallback cannot shift later connectors.
  Suspicious cases are confirmed with the same script's terminal slideshow,
  because the real terminal remains the final glyph-fidelity oracle.
- `cargo test` must stay < 5s.

### Quality guarantee model

The integer grid is the measurement space, not by itself an aesthetic
guarantee. Quality comes from a semantic, independently measured loop:

```text
diagram IR
    -> constraint-based layout
    -> final normalized Scene
    -> independent semantic predicates + checked raster
    -> pass / fail / not-applicable + exact witnesses
```

The guarantees have deliberately different scopes:

1. Determinism, integer coordinates, and non-truncation are engine-wide
   properties. `scene.integrity` independently checks the actual final Scene
   and raster rather than trusting layout-owned bookkeeping.
2. Semantic fidelity and aesthetic preferences are relational. Diagram IR
   supplies identity and intent; the normalized final Scene supplies measured
   coordinates. Checks cover declared endpoints, structured compartments,
   group containment, chronology, parent-child spans, chains, forks, merges,
   eligible diamonds, message/lifeline alignment, connector padding, and
   similar relationships using exact integer or doubled-cell witnesses.
3. Every check has a class (`invariant`, `preference`, or `budget`), an
   applicability count, and a `pass`, `fail`, or `not_applicable` status. A
   composition without a sound predicate is recorded as `unclassified`; it is
   never silently counted as a success. Budget overflow remains visible but is
   distinct from fidelity failure because B9 permits honest over-width output.
4. Reviewed goldens prove representative compositions and prevent regressions.
   `tests/inspection.rs` gates all applicable invariants and preferences over
   that corpus. Broad generated corpora gate invariants and exercise the
   classifier without pretending every arbitrary topology has a formal beauty
   definition.
5. Human review discovers preferences that have not yet been formalized. An
   accepted preference becomes a minimal fixture plus a failing named geometry
   contract before the layout rule changes; the golden is updated only after
   the generalized rule passes the whole corpus.

`--inspect=json` exposes this model directly, including the raw normalized grid
and exact canvas rows for forensic use. A fixed expected-placement grid is not
the primary oracle: it would reject harmless translations or equally valid
spacing choices and would duplicate snapshots at coordinate level. Relational
predicates say what must remain true while leaving layout freedom. New
preferences still enter through a topology-specific predicate and minimized
fixture before receiving a diagnostic name. This can guarantee every
preference with an exact geometric definition; subjective beauty and terminal
font fidelity still require the review loop. `--audit=json` remains the compact
backward-compatible metric report for existing consumers.

## Milestones

1. **M1**: parser + IR + golden-test harness (parse-only snapshots)
2. **M2**: layout ranks/ordering; render boxes + straight edges — LR pipeline renders
3. **M3**: elbows, edge labels, fork/merge, TB — the session's reference diagrams render
4. **M4**: shapes, cycles/self-loops, `--ascii`, `--width`, error messages — v1 complete
