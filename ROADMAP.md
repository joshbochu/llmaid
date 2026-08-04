# llmaid — roadmap

**Product stance:** agents drive creation and self-debug; humans look at the
visuals. Mermaid in, glanceable deterministic terminal out.

**How to use this doc**
- Phases are ordered for that stance (quality before breadth).
- Coverage detail lives in [`MATRIX.md`](MATRIX.md) — update both when a
  feature lands (checklist cell + phase status).
- Behavior contracts for shipped promises: [`BEHAVIORS.md`](BEHAVIORS.md).
- Day-to-day agent pickup: [`HANDOFF.md`](HANDOFF.md).

Legend: **done** · **next** · **later** · **park**

---

## North star

One binary agents can call for **most Mermaid they emit**, at a quality bar a
human can trust at a glance (no truncation, labels on structure, closed frames,
agent-fixable errors).

Internal model: Mermaid → type-specific IR/layout/router → shared signed Scene → canvas/style.
Agents speak Mermaid; we own primitives behind the door.

---

## Parity targets

- **Strategic parity target:** sequence diagrams.
- **Quickest parity target:** timeline.

---

## Phase status

| Phase | Name | Status |
|-------|------|--------|
| — | v1 flowchart baseline (B1–B14) | **done** |
| 0 | Ship & tighten flowchart | **done** |
| 1 | Flowchart completeness (subgraphs / graph-easy parity) | **done** (1.1 flat; nested parent field ready) |
| 2 | Sequence diagrams | **done** (core + controls) |
| 3 | Design-doc types (state / class / ER) | **done** (core + visual fidelity) |
| 4 | Hierarchy & planning (mindmap / timeline) | **done for current scope** |
| 5 | Charts & boards (selective) | **parked** |
| 6 | Agent self-debug loop | **in progress** (6.2–6.3, 6.5, and 6.7 done) |
| 7 | Distribution & product | **in progress** (README/CI/package metadata/license done; channel pending) |

---

## Done — v1 baseline

Flowchart-only terminal Mermaid with contracts B1–B14:

- Parse → layout → render; LR/RL/TB/BT
- On-arrow labels, shape hints, cycles/self-loops, parallel ports
- `--ascii`, `--width` overflow ladder, stdout purity, fixed default width
- Goldens (`.ir` / `.txt`) + canvas invariants

See `BEHAVIORS.md`, `CHANGELOG.md`.

---

## Phase 0 — Ship & tighten flowchart · **done**

| ID | Item | Notes |
|----|------|--------|
| 0.1 | Label-spacing polish (` scan `) + goldens | Shipped |
| 0.2 | RL/BT explicit golden cases | `dir-rl`, `dir-bt` + behavior test |
| 0.3 | TB on-arrow edge labels | Beside vertical shaft; `dir-tb-labels` |
| 0.4 | Simple-chain port alignment | Mono-rank straighten + mono-edge port snap |
| 0.5 | Tighter TB channels / multi-edge cases | Less vertical waste; edge-labels still complex but labeled |

**Exit met:** directions covered; TB labels visible; simple TB chains straight.

---

## Phase 1 — Flowchart completeness · **done** (core)

Closest to **graph-easy** parity inside Mermaid.

| ID | Item | Notes |
|----|------|--------|
| 1.1 | Real **subgraphs** (title + border + containment) | **done** — parse + bbox + frame |
| 1.2 | Nested subgraphs (if tractable) | parent stack works; polish later if needed |
| 1.3 | Subgraph-safe edge routing | **done** — edges may cross frames and declared subgraph IDs attach to their semantic frames without phantom nodes |
| 1.4 | Core terminal endpoint notation | **done** — flowchart circles, crosses, and bidirectional arrows lower as semantic endpoint decorations with inspected geometry |
| 1.5 | Optional terminal-safe styling subset | Still deferred |

**Exit met:** titled group frames around members; B15 + `subgraph-basic` / `subgraph-lr` goldens.

---

## Phase 2 — Sequence diagrams · **done** (core + controls)

Highest-value second type (agents + **diagon seq**).

| ID | Item | Notes |
|----|------|--------|
| 2.1 | `sequenceDiagram` parse → IR | **done** — participants/actors, implicit participants, `->>` / `-->>` |
| 2.2 | Layout: actors, lifelines, messages, `-->>` returns | **done** — first end-to-end shared-Scene slice |
| 2.3 | Notes / activate (core subset) | **done** — left/right/over notes + balanced explicit activation bars |
| 2.4 | Goldens + invariants + agent errors | **done** — core, notes, activation, ordering, and malformed-input coverage |
| 2.5 | Control blocks: `loop` / `alt` / `opt` | **done** — balanced nesting, `else`, framed Unicode/ASCII output |

**Exit met:** API/protocol diagrams are viewable and self-debuggable; B17/B18/B20
plus `sequence-core`, `sequence-notes`, `sequence-activation`, and
`sequence-blocks` goldens.

---

## Phase 3 — Design-doc types · **done** (core + visual fidelity)

Boxed-graph family; reuse canvas + much of edge drawing.

| ID | Item | Notes |
|----|------|-------|
| 3.1 | `stateDiagram` / `stateDiagram-v2` (flat first, nested later) | **done** — aliases, markers, labels, directions |
| 3.2 | `classDiagram` | **done** — members, relations, multiplicities |
| 3.3 | `erDiagram` | **done** — attributes, keys, cardinalities |
| 3.4 | Native terminal visual grammar | **done** — compartments, tables, spaced UML/ER endpoint adornments |

**Exit met:** design conversations have a visual home in the terminal; B21–B23
plus state/class/ER goldens, generated direction coverage, and exact endpoint /
structured-box quality contracts.

---

## Phase 4 — Hierarchy & planning · **done for current scope**

| ID | Item | Maps from |
|----|------|-----------|
| 4.1 | `mindmap` + reusable ordered tree layout | **done** — one root, plain descendants, exact geometry, audit, goldens |
| 4.2 | `timeline` + reusable integer temporal layout | **done** — title, periods/events, sections, audit, exact geometry, generated coverage |
| 4.3 | `gitGraph` | **parked** — tested slice retained on `codex/git-todo-later` |

**Exit:** hierarchy and chronological planning narratives without another
specialty engine.

---

## Phase 5 — Charts & boards (selective) · **parked**

The shipped core covers the primary agent communication use cases. Additional
chart and board breadth stays parked until concrete usage justifies its layout
and maintenance cost.

| ID | Item | Priority |
|----|------|----------|
| 5.1 | `pie` | parked |
| 5.2 | `xychart` / `quadrantChart` / `treemap` | parked |
| 5.3 | `journey` / `kanban` / `block-beta` | parked |

**Exit:** multi-type without sacrificing the visual bar.

---

## Phase 6 — Agent self-debug

| ID | Item | Notes |
|----|------|--------|
| 6.1 | Richer per-type error catalog | **in progress** — unified source + line + excerpt + expectation diagnostics; known unsupported headers fail directly |
| 6.2 | Machine-readable geometry audit | **done** — stable v1 JSON; named violations, exact witnesses, every shipped type |
| 6.3 | Generated + metamorphic topology tests | **done** — 71 DAGs + 40 design-doc direction renders + 197 ordered tree shapes + 170 timeline structures |
| 6.4 | Optional machine-readable parser diagnostics | e.g. `--error-format=json` |
| 6.5 | Invariant failures as actionable stderr | **done** — checked runtime render, exit 70, inspection hint |
| 6.6 | Optional JSON IR **input** (same engines) | Tools/compilers; Mermaid remains primary |
| 6.7 | Semantic final-Scene inspection | **done** — stable `llmaid.inspect.v1`; typed applicability/status/witnesses, semantic geometry, raster rows, reviewed-gallery preference gate, generated invariant gates, and mutation tests |
| 6.8 | Deterministic resource bounds | **done** — capped source streaming, semantic/depth limits, checked/fallible canvas allocation, and bounded inspection refusal |

**Exit:** an agent can render any input, inspect exact structural violations,
and fix failures in one retry without a human reading either the parser or the
diagram. Generated topology coverage guards combinations absent from goldens;
unclassified compositions and subjective preferences remain explicit inputs
to the human review loop rather than inferred machine passes.

---

## Phase 7 — Distribution & product

| ID | Item |
|----|------|
| 7.0 | **done** — MIT license declared in the package and repository |
| 7.1 | **done** — crates.io selected as the first release channel (`cargo install llmaid`) |
| 7.2 | Versioned CLI; examples per diagram type in `--help` or docs |
| 7.3 | Visual gallery (checked-in renders) for human taste QA |
| 7.4 | Optional wasm/npm embed |

---

## Park (explicit non-goals for now)

| Item | Why |
|------|-----|
| `gitGraph` | Useful but non-core; tested implementation retained on `codex/git-todo-later` at `7f2989b` |
| `pie` / `xychart` / `quadrantChart` / `treemap` | Colorless terminal grammar and real demand are not yet strong enough |
| `journey` / `kanban` / `block-beta` | Lower-value breadth; overlaps existing timeline, flowchart, and document workflows |
| Full Mermaid.js theme/color parity | Fights terminal + determinism |
| Interactive TUI as core product | Humans look at static output; agents don’t need a TUI |
| Diagon **math** formulas | Not Mermaid; separate tool if ever |
| Diagon **grammar** railroads | Niche; no good Mermaid home |
| First-class Mermaid **tables** | Markdown tables already win |
| Human drag-and-drop editor | Wrong product |

---

## Engine map (implementation lens)

| Engine | Unlocks phases |
|--------|----------------|
| Layered digraph + clusters | 0, 1 (flowchart + subgraphs) |
| Sequence / lifelines | 2 |
| Typed box graphs | 3 (state/class/ER variants) |
| Tree | 4.1 |
| Time axis | 4.2 (**timeline core done**) |
| Specialty (git, charts) | 4.3, 5 |
| Shared Scene / canvas / style / width / invariants | all |

---

## Working rules

1. **Mermaid in** for agent-facing input; primitives only as internal IR (or optional advanced input).
2. **No truncation**; stdout = diagram only; errors name line + expectation.
3. New type ships with goldens + at least one behavior/invariant check.
4. Update **MATRIX.md** when coverage changes; mark phase **done** here when exit criteria met.
5. Prefer glance quality over checkbox parity with termaid.
6. A visual preference becomes enforceable only when stated as an applicable
   exact geometry predicate; the grid supplies coordinates, not a beauty score.

---

## Suggested near-term sequence

```text
gallery-driven polish of shipped types
→ 6.1/6.4 agent diagnostics
→ 7.2–7.3 versioned docs and visual gallery
```
