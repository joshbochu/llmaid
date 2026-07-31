# Semantic inspection

`llmaid --inspect=json` is the agent-facing self-verification interface. It
answers two different questions in one deterministic document:

1. What was actually painted? The report includes normalized final-Scene
   geometry and the exact terminal canvas rows.
2. Which declared quality relationships were checked? The report includes
   typed predicates, applicability, status, semantic elements, and exact
   witnesses.

The grid is evidence, not a fixed placement template. Absolute coordinate
snapshots are useful for byte regression tests, but they overconstrain harmless
translations and alternative valid spacing. Inspection therefore checks
semantic relationships—attachment, containment, ordering, centering,
symmetry, padding, and table structure—against the final Scene.

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

## `llmaid.inspect.v1`

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
- sequence header/lifeline alignment, message ordering and centering, and
  fragment span;
- state transition endpoints and node shapes;
- class relation endpoints, compartments, endpoint decorations, and
  multiplicities;
- ER relationship endpoints, attribute tables, and cardinalities;
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

To formalize a new preference:

1. Minimize a visually bad composition.
2. State the eligible semantic topology and exact integer relationship.
3. Add a final-Scene mutation or failing contract that names its elements and
   witness.
4. Implement the predicate, marking other compositions not applicable or
   unclassified.
5. Improve layout, run generated and reviewed-gallery gates, then update a
   golden only when the new output has been visually accepted.

Never replace these predicates with a global beauty score. A scalar hides which
relationship failed and makes unrelated layout tradeoffs impossible to audit.
