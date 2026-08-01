# Render verification: design, mechanics, and agent workflow

This guide is for people who want to understand how llmaid verifies a render,
and for coding agents that need to use that verification correctly.

## Short version

A person or model should not have to answer “does this diagram look right?”
from intuition alone. `llmaid --inspect=json` turns the completed render into a
deterministic inspection report. It provides:

- a ruler: exact terminal-cell geometry and the final canvas rows;
- a checklist: named structural and aesthetic relationships that pass or fail;
- a diagnosis: affected semantic elements and exact integer witnesses; and
- an honesty mechanism: unsupported aesthetic compositions are explicitly
  `unclassified`, never silently treated as good.

This does not make all visual taste computable. It makes every currently
declared claim mechanically verifiable and exposes the remaining judgment
boundary. A human can inspect the same `canvas.rows` that the checks measure.

## Why we built it this way

Text snapshots answer whether output changed, but not whether the new output is
better. An LLM looking at a diagram can notice many problems, but its judgment
is non-deterministic and it may miss a one-cell error or rationalize a bad
result. A single “beauty score” has the opposite problem: it hides which
relationship failed and lets unrelated tradeoffs cancel each other out.

Inspection instead answers two concrete questions in one stable document:

1. What was actually painted? The report includes normalized final-Scene
   geometry and exact terminal rows.
2. Which declared relationships were checked? The report includes typed
   predicates, applicability, status, semantic elements, and exact witnesses.

The grid is evidence, not a fixed placement template. Absolute coordinates are
useful for byte regression tests, but they overconstrain harmless translations
and alternative valid spacing. Inspection checks relationships—attachment,
containment, ordering, centering, symmetry, padding, and table structure—on
the final Scene instead.

## How the implementation works

```text
Mermaid source
    │
    ▼
semantic parser ── records what nodes, edges, messages, fields, and events mean
    │
    ▼
layout + routing ── chooses integer terminal-cell geometry
    │
    ▼
normalized final Scene
    ├──▶ renderer ───────────────▶ exact canvas rows
    └──▶ independent evaluator ──▶ checks + witnesses + unclassified cases
                                      │
                                      ▼
                              llmaid.inspect.v1 JSON
```

Each diagram engine has its own semantic representation and layout rules, but
all engines lower into the same `Scene`: boxes, groups, routed edges, paths,
decorations, and text in signed integer coordinates. `Scene::normalize` then
translates every primitive once so the measured geometry and terminal canvas
share the same origin.

The evaluator receives two inputs: semantic intent from the parsed diagram and
observed geometry from the normalized final Scene. It deliberately does not
consult layout-engine intermediate calculations. For example, the parser says
which two participants a message connects; the final Scene says where its
attachment cells and label were actually painted. The evaluator compares those
two views. This is the practical meaning of “not grading its own homework.”

Finally, the inspection serializer combines semantic identities, geometry,
check results, and raster rows in stable field and declaration order. The same
input and flags therefore produce byte-identical inspection JSON.

### Source map

- `src/diagram.rs` detects the diagram type and sends every engine to the
  shared `Scene` boundary.
- `src/scene.rs` defines the geometry primitives, exact bounds, doubled
  centers, semantic node handles, and the one normalization translation.
- `src/render.rs` paints that Scene and performs raster-level integrity checks.
- `src/quality.rs` compares parsed semantic intent with final Scene evidence
  and produces typed checks, failures, witnesses, and unclassified cases.
- `src/inspect.rs` assigns stable semantic names and serializes geometry,
  quality results, and canvas rows as `llmaid.inspect.v1`.
- `tests/inspection.rs`, generated suites, and mutation tests in
  `src/quality.rs` verify reviewed quality, broad invariants, determinism, and
  evaluator independence.

## What each result means

| Result | Plain-English meaning | Required response |
| --- | --- | --- |
| Invariant failure | The render is structurally or semantically unsafe. | Block publication and fix it. |
| Preference failure | An exact aesthetic rule failed for an eligible topology. | Fix it or explicitly review the tradeoff. |
| Budget failure | Correct content exceeds the requested width. | Widen, wrap, shorten author-controlled text, split, or knowingly accept it. |
| `unclassified` | No sound exact aesthetic predicate exists yet for this composition. | Treat quality as unknown and visually review it. |
| `not_applicable` | A check does not apply to this topology. | Draw no quality conclusion from that check. |
| Pass | The named relationship was applicable and held exactly. | Proceed for that claim only. |

These categories prevent a width compromise from masquerading as corruption,
and prevent missing coverage from masquerading as success.

## What “quantitative” means here

Every classified pass or failure is exact and repeatable. Geometry uses integer
terminal cells. Rectangle centers use doubled coordinates
(`2 * origin + extent - 1`) so half-cell parity is retained without floats.
Predicates use exact equality, ordering, containment, adjacency, padding, and
identity comparisons. Examples include:

- a centering residual must equal zero;
- a label must have at least one clear padding cell;
- message rows must remain in semantic source order;
- an edge endpoint must equal an allowed border attachment cell;
- a structured class row must equal the parsed class member; and
- width overflow is the rendered width minus the requested target.

A failed check records the semantic elements and measured witness rather than
returning a fuzzy percentage. There is deliberately no global “82% beautiful”
score.

## Agent loop

```sh
llmaid --inspect=json --width 100 diagram.mmd
```

Read the result in this order:

1. Require `summary.invariant_failed_checks == 0`. An invariant failure means
   the rendered result is structurally or semantically unsafe to publish.
2. Inspect `summary.quality_failed_checks`. This includes invariant and
   preference failures but excludes permitted width-budget overflow.
3. Read each failed check's `elements`, `message`, and `witness` fields. They
   identify the semantic relationship and exact integer discrepancy; do not
   infer the problem from a screenshot alone.
4. Treat entries in `unclassified` as unknown quality, not success. Minimize
   and review that composition if its appearance matters.
5. Inspect `viewport.width` separately. B9 deliberately prefers honest
   over-width output to label truncation; an agent can increase `--width`, wrap
   or shorten author-controlled labels, or split the diagram.
6. Use `canvas.rows` for the same final visual the human will see. Confirm
   Unicode glyph fidelity in a real terminal when a font-dependent issue is
   suspected.

`--ascii` changes both the normal render and `canvas.rows`, while all semantic
geometry and checks remain in terminal-cell coordinates. `--audit=json` is a
smaller compatibility surface and cannot be combined with `--inspect=json`.

## Worked examples

- **A node moves two rows off a simple flow centerline.**
  `flow.mono_centerline` fails, names the involved nodes, and records the center
  discrepancy. The layout can be adjusted and inspected again.
- **A class box paints the wrong member.** `class.compartments` fails for that
  semantic class even if the border still looks intact. This catches fidelity,
  not merely raster damage.
- **A long identifier makes the canvas wider than requested.**
  `viewport.width` reports a budget failure while invariants can remain clean.
  The diagram is honest, but it does not fit the requested target.
- **A many-to-many flow composition has no sound centering rule.** The relevant
  fork and merge compositions appear in `unclassified`. A person or agent must
  review the canvas instead of claiming a machine-proven aesthetic pass.
- **A sequence label is one cell left of its true midpoint.** An exact message
  centering predicate exposes the residual. This work found that real defect;
  the layout now uses the occupied text extent rather than the display width
  when calculating its center.

## A concrete end-to-end example

For a simple `A --> B` flowchart, parsing establishes two semantic node
identities and one directed relationship. Layout creates two boxes; routing
creates a path and arrow; normalization moves the complete Scene to `(0, 0)`.
The evaluator then verifies, among other things, that the routed edge attaches
to the boxes and that this eligible one-edge topology shares a centerline. The
serializer emits those checks alongside box rectangles, doubled centers, edge
points, arrow direction, and the exact three terminal rows.

If `B` is moved after layout but before evaluation, the semantic relationship
is unchanged while the observed centerline is not. The corresponding check
fails. That separation between expected intent and observed final geometry is
what makes the report useful as independent evidence.

## `llmaid.inspect.v1`

A complete small report is committed at
[`tests/fixtures/simple.inspect.json`](tests/fixtures/simple.inspect.json).

Top-level fields are emitted in stable order:

- `schema`, `diagram`, `style`, and normalized `bounds` identify the artifact.
- `summary` contains total checks and applicable instances, all failed checks,
  invariant failures, invariant-or-preference quality failures, and the count
  of unclassified compositions.
- `checks` contains `id`, `class`, `status`, `applicable`, and `failures`.
  Status is `not_applicable` only when the applicability count is zero.
- `unclassified` names compositions for which no sound exact predicate exists.
- `geometry` contains semantic boxes, groups, paths, routed edges, endpoint
  decorations, and free texts. Rectangles also expose doubled centers so
  half-cell parity is never lost.
- `canvas` contains exact width, height, and trailing-whitespace-free rows.

Check classes have different meanings:

- `invariant`: structural or semantic fidelity that must always hold.
- `preference`: an exact aesthetic relationship, applied only to eligible
  topology.
- `budget`: a requested fit target whose failure can be legitimate.

Current check families include:

- shared scene integrity and viewport width;
- flowchart endpoint attachment, simple centerlines, exclusive fork/merge
  centering, eligible-diamond mirroring, unrelated crossings, and groups;
- sequence header/lifeline alignment, message ordering and centering, fragment
  span, and final-frame lifeline termination;
- state transition endpoints and node shapes;
- class relation endpoints, compartments, endpoint decorations, and
  multiplicities;
- ER relationship endpoints, attribute tables, cardinalities, relationship
  label attachment, and shared-endpoint lane separation;
- mindmap parent/child spans, edge attachments, padding, and depth columns;
- timeline spine, chronology, connector padding, section containment, and
  title centering.

Feedback routes, self-loops, overlapping flow junctions, boxed-graph layout
composition, and note/activation spacing are examples of explicitly
unclassified compositions. Their correctness is still covered by Scene
invariants; their complete aesthetics are not claimed.

## Test strategy

The project uses three complementary gates:

- `tests/inspection.rs` requires every applicable invariant and preference to
  pass over all human-reviewed golden cases and verifies byte-stable reports.
- Generated flowchart, state/class/ER, mindmap, and timeline suites require all
  semantic invariants to pass across broad deterministic input spaces.
- Unit mutation tests alter only final Scene geometry and require the
  independent evaluator to fail with a named witness. Exact layout changes
  continue to receive topology-specific contracts in `tests/quality.rs`.

These layers answer different questions. Goldens preserve examples whose
appearance people have accepted. Generated cases search a broader,
deterministic input space for structural failures. Mutations prove that the
evaluator actually notices damage: a test changes the final geometry without
changing semantic intent and requires a named check to fail. Passing the
renderer’s ordinary output is not sufficient evidence unless damaged output is
also known to be rejected.

The evaluator itself is still software and can contain mistakes. Its defenses
are small named predicates, exact witnesses, deterministic inputs, mutation
tests, and the retained human-facing raster. A failed or surprising result can
therefore be traced to one relationship rather than reverse-engineered from a
combined score.

To formalize a new preference:

1. Minimize a visually bad composition.
2. State the eligible semantic topology and exact integer relationship.
3. Add a final-Scene mutation or failing contract that names its elements and
   witness.
4. Implement the predicate, marking other compositions not applicable or
   unclassified.
5. Improve layout, run generated and reviewed-gallery gates, then update a
   golden only when the new output has been visually accepted.

## Limits and human review

Inspection can prove only declared relationships. It does not yet prove every
possible judgment about density, balance, readability, font rendering, or
whether a diagram is the best explanation for its audience. Terminal font and
glyph behavior also requires a real terminal when fidelity is in doubt.

That boundary is intentional:

- exact claims become predicates;
- ineligible checks become `not_applicable`;
- unsupported compositions become `unclassified`; and
- subjective or font-dependent concerns stay in the gallery review loop.

This lets people and agents automate safe decisions without pretending that
all visual design has been reduced to mathematics.

Never replace these predicates with a global beauty score. A scalar hides which
relationship failed and makes unrelated layout tradeoffs impossible to audit.
