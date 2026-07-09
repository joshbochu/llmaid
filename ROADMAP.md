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

Internal model: Mermaid → typed IR → layout engines → shared canvas/style.
Agents speak Mermaid; we own primitives behind the door.

---

## Phase status

| Phase | Name | Status |
|-------|------|--------|
| — | v1 flowchart baseline (B1–B14) | **done** |
| 0 | Ship & tighten flowchart | **next** |
| 1 | Flowchart completeness (subgraphs / graph-easy parity) | later |
| 2 | Sequence diagrams | later |
| 3 | Design-doc types (state / class / ER) | later |
| 4 | Hierarchy & planning (mindmap / gantt / git) | later |
| 5 | Charts & boards (selective) | later |
| 6 | Agent self-debug loop | later |
| 7 | Distribution & product | later |

---

## Done — v1 baseline

Flowchart-only terminal Mermaid with contracts B1–B14:

- Parse → layout → render; LR/RL/TB/BT
- On-arrow labels, shape hints, cycles/self-loops, parallel ports
- `--ascii`, `--width` overflow ladder, stdout purity, fixed default width
- Goldens (`.ir` / `.txt`) + canvas invariants

See `BEHAVIORS.md`, `CHANGELOG.md`.

**Pending commit (polish on tree):** edge labels padded as ` scan ` (space each
side) — land before Phase 0 closes.

---

## Phase 0 — Ship & tighten flowchart · **next**

| ID | Item | Notes |
|----|------|--------|
| 0.1 | Commit label-spacing polish + refresh `.txt` goldens | Aesthetic win in working tree |
| 0.2 | RL/BT explicit golden cases | Mirroring implemented; under-tested |
| 0.3 | TB on-arrow edge labels | Today LR-focused |
| 0.4 | Optional: simple-chain port alignment | Fewer needless jogs on pure A→B chains |
| 0.5 | Dense multi-edge aesthetic pass | Quality bar: `tests/cases/edge-labels.mmd` |

**Exit:** every reference flowchart is glance-trusted; Phase 0 rows in MATRIX green where applicable.

---

## Phase 1 — Flowchart completeness

Closest to **graph-easy** parity inside Mermaid.

| ID | Item | Notes |
|----|------|--------|
| 1.1 | Real **subgraphs** (title + border + containment) | Today: warn + flatten |
| 1.2 | Nested subgraphs (if tractable) | Architecture diagrams |
| 1.3 | Subgraph-safe edge routing | Edges in/out of groups |
| 1.4 | Optional terminal-safe styling subset | Not a theme zoo; maybe ignore color still |

**Exit:** systems / architecture Mermaid flowcharts render as groups, not flat soup.

---

## Phase 2 — Sequence diagrams

Highest-value second type (agents + **diagon seq**).

| ID | Item | Notes |
|----|------|--------|
| 2.1 | `sequenceDiagram` parse → IR | Core messages first |
| 2.2 | Layout: actors, lifelines, messages, dashed returns | Shared canvas |
| 2.3 | Notes / activate (core subset) | Expand after goldens |
| 2.4 | Goldens + invariants + agent errors | Same bar as flowcharts |

**Exit:** API/protocol diagrams are viewable and self-debuggable.

---

## Phase 3 — Design-doc types

Boxed-graph family; reuse canvas + much of edge drawing.

| ID | Item |
|----|------|
| 3.1 | `stateDiagram` / `stateDiagram-v2` (flat first, nested later) |
| 3.2 | `classDiagram` |
| 3.3 | `erDiagram` |

**Exit:** design conversations have a visual home in the terminal.

---

## Phase 4 — Hierarchy & planning

| ID | Item | Maps from |
|----|------|-----------|
| 4.1 | `mindmap` (and/or tree layout) | diagon tree |
| 4.2 | `gantt` or `timeline` | planning |
| 4.3 | `gitGraph` | branch explainers |

**Exit:** planning + repo narrative diagrams.

---

## Phase 5 — Charts & boards (selective)

Only where glance quality holds (don’t become a junk drawer).

| ID | Item | Priority |
|----|------|----------|
| 5.1 | `pie` | higher |
| 5.2 | `xychart` / `quadrantChart` / `treemap` | medium |
| 5.3 | `journey` / `kanban` / `block-beta` | lower |

**Exit:** multi-type without sacrificing the visual bar.

---

## Phase 6 — Agent self-debug

| ID | Item | Notes |
|----|------|--------|
| 6.1 | Richer per-type error catalog | Line + expectation everywhere |
| 6.2 | Optional machine-readable diagnostics | e.g. `--error-format=json` |
| 6.3 | Invariant failures as actionable stderr | Not only test-time |
| 6.4 | Optional JSON IR **input** (same engines) | Tools/compilers; Mermaid remains primary |

**Exit:** agent fixes failures in one retry without a human reading the parser.

---

## Phase 7 — Distribution & product

| ID | Item |
|----|------|
| 7.1 | Release channel (crates.io / brew / gh releases — pick one first) |
| 7.2 | Versioned CLI; examples per diagram type in `--help` or docs |
| 7.3 | Visual gallery (checked-in renders) for human taste QA |
| 7.4 | Optional wasm/npm embed |

---

## Park (explicit non-goals for now)

| Item | Why |
|------|-----|
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
| Time axis | 4.2 |
| Specialty (git, charts) | 4.3, 5 |
| Shared canvas / style / width / invariants | all |

---

## Working rules

1. **Mermaid in** for agent-facing input; primitives only as internal IR (or optional advanced input).
2. **No truncation**; stdout = diagram only; errors name line + expectation.
3. New type ships with goldens + at least one behavior/invariant check.
4. Update **MATRIX.md** when coverage changes; mark phase **done** here when exit criteria met.
5. Prefer glance quality over checkbox parity with termaid.

---

## Suggested near-term sequence

```text
0.1 commit spacing → 0.2–0.5 flowchart polish
→ 1.x subgraphs
→ 2.x sequence
→ 3.x state/class/ER
→ 4.x mindmap/gantt/git
→ 5.x charts (selective)
→ 6.x agent diagnostics
→ 7.x distribute
```
