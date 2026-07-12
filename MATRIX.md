# llmaid — coverage matrix

Living checklist of **visual/capabilities** vs tools. Update when features land
or competitor notes change. Roadmap phases: [`ROADMAP.md`](ROADMAP.md).

**Product:** agents author Mermaid and self-debug; humans judge the picture.

## Legend

| Mark | Meaning |
|------|---------|
| **Y** | Supported well enough to use |
| **P** | Partial / limited / quality gaps |
| **N** | Not supported |
| **—** | Not applicable (different product shape) |
| **?** | Unverified / version-dependent |

**Columns**

| Tool | Role in comparison |
|------|-------------------|
| **llmaid** | This project (flowchart + sequence + core design-doc, mindmap, and timeline subsets) |
| **termiflow** | Terminal Mermaid flowchart (`tw`) |
| **termaid** | Terminal multi-type Mermaid (Python) |
| **diagon** | Non-Mermaid ASCII generators |
| **graph-easy** | Non-Mermaid graph language → ASCII/boxart/… |
| **mermaid.js** | Full Mermaid reference (browser/SVG; not terminal) |

Marks for competitors are **approximate** (local probes + public surface area),
not a formal audit. llmaid cells should stay honest.

---

## 1. Input languages

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Mermaid flowchart/graph | Y | Y | Y | N | N | Y |
| Mermaid sequence | P | N | Y | N | N | Y |
| Mermaid state | P | N | Y | N | N | Y |
| Mermaid class | P | N | Y | N | N | Y |
| Mermaid ER | P | N | Y | N | N | Y |
| Mermaid mindmap | P | N | Y | N | N | Y |
| Mermaid gantt / timeline | P (timeline) | N | Y | N | N | Y |
| Mermaid gitGraph | P | N | Y | N | N | Y |
| Mermaid pie / charts | N | N | Y | N | N | Y |
| Mermaid journey / kanban / block | N | N | Y | N | N | Y |
| Mermaid subgraph (real layout) | Y | ? | P | N | — | Y |
| graph-easy text / DOT in | N | N | N | N | Y | N |
| diagon mini-languages | N | N | N | Y | N | N |
| JSON graph schema in | N | Y | P | N | N | N |
| Agent-native (pretraining) | Y | Y | Y | N | N | Y |

**llmaid target:** grow Mermaid columns toward mermaid.js/termaid breadth; keep
Mermaid as the only primary agent language.

---

## 2. Flowchart / layered digraph primitives

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Nodes + labels | Y | Y | Y | Y (dag) | Y | Y |
| Directed edges | Y | Y | Y | Y | Y | Y |
| Edge labels | Y | P | P | P | Y | Y |
| Labels **on** edge shaft | Y | N | P | P | Y | — |
| Solid / dotted / thick edges | Y | Y | Y | P | Y | Y |
| LR / RL / TB / BT | Y | Y | Y | P | Y | Y |
| TB/BT edge labels on shaft | Y | ? | P | — | P | Y |
| RL edge labels | Y | ? | P | — | P | Y |
| Fork / merge | Y | Y | Y | Y | Y | Y |
| Cycles / back-edges | Y | P | P | N (dag) | Y | Y |
| Self-loops | Y | P | P | N | Y | Y |
| Parallel edges (multi-edge) | Y | N | Y | P | Y | Y |
| Node shapes (rect family) | Y | Y | Y | P | Y | Y |
| Shape fidelity vs grid | P (hints) | P | P | P | P | Y (SVG) |
| Subgraphs / clusters | Y | ? | P | N | Y | Y |
| Nested groups | P | N | P | N | Y | Y |
| Fan-out `&` | Y | ? | ? | N | P | Y |
| Never truncate labels | Y | N | P | Y | Y | Y |
| Orthogonal elbows | Y | Y | Y | Y | Y | Y |

**Roadmap:** Phase 0 polish · Phase 1 subgraphs.

---

## 3. Sequence / interaction primitives (diagon `seq`)

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Participants / actors | Y | N | Y | Y | N | Y |
| Lifelines | Y | N | Y | Y | N | Y |
| Sync / async messages | P | N | Y | Y | N | Y |
| Dashed return | Y | N | Y | Y | N | Y |
| Notes | Y | N | Y | P | N | Y |
| Activate / deactivate | Y | N | P | P | N | Y |
| Loops / alt / opt | Y | N | P | P | N | Y |

**Roadmap:** Phase 2.

---

## 4. Tree / hierarchy (diagon `tree`)

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Indented / box tree | Y | N | Y (mindmap) | Y | P (as graph) | Y (mindmap) |
| Parent–child only layout | Y | N | Y | Y | P | Y |

**Roadmap:** Phase 4.1.

---

## 5. Design-doc boxed graphs

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| State machines | P | N | Y | N | P | Y |
| Class diagrams | P | N | Y | N | P | Y |
| ER diagrams | P | N | Y | N | P | Y |
| Class compartments + UML endpoints | Y | N | P | N | P | Y |
| ER attribute tables + endpoint cardinality | Y | N | P | N | P | Y |

**Roadmap:** Phase 3. (graph-easy can fake some via shapes; not first-class.)

---

## 6. Planning / git

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Gantt / timeline | P (timeline) | N | Y | N | N | Y |
| Git graph | P (commits, branches, merges) | N | Y | N | N | Y |

**Roadmap:** Phase 4.2–4.3.

---

## 7. Charts & boards

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Pie | N | N | Y | N | N | Y |
| XY / bar-like | N | N | Y | N | N | Y |
| Quadrant | N | N | Y | N | N | Y |
| Treemap | N | N | Y | N | N | Y |
| User journey | N | N | Y | N | N | Y |
| Kanban | N | N | Y | N | N | Y |
| Block / C4-like | N | N | Y | N | N | Y |

**Roadmap:** Phase 5 (selective).

---

## 8. Non-Mermaid specialty (diagon-only-ish)

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Math formula layout | N | N | N | Y | N | N |
| ASCII table generator | N | N | N | Y | N | N |
| Frame / callout box | N | N | N | Y | P | P |
| Grammar / railroad | N | N | N | P | N | N |

**Roadmap:** mostly **park** (not Mermaid). Optional side modes only if needed.

---

## 9. Terminal / agent product qualities

| Capability | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Terminal Unicode/ASCII out | Y | Y | Y | Y | Y | N (SVG) |
| Single fast static binary | Y | Y | N (Py) | P | N (Perl) | N |
| Deterministic bytes (no TTY width) | Y | P | P | Y | Y | — |
| Fixed `--width` / fit ladder | Y | P | Y | N | N | — |
| stdout = diagram only | Y | P | P | Y | Y | — |
| Parse errors: line + expectation | Y | P | P | P | P | Y |
| Never truncate labels | Y | N | P | Y | Y | Y |
| Behavior contracts / goldens | Y | ? | ? | N | N | — |
| Frame invariants (tests) | Y | ? | ? | N | N | — |
| Machine geometry audit | Y | N | N | N | N | — |
| Generated/metamorphic coverage | Y | ? | ? | N | N | — |
| Color themes | N | P | Y | N | P | Y |
| Interactive TUI | N | P | Y | N | N | Y |

**Roadmap:** Phase 6 (diagnostics) · Phase 7 (distribute). Stay strong on agent columns.

---

## 10. Primitive engines (language-agnostic)

What layout *kind* each tool implements — independent of syntax.

| Engine / primitive family | llmaid | termiflow | termaid | diagon | graph-easy | mermaid.js |
|---------------------------|:------:|:---------:|:-------:|:------:|:----------:|:----------:|
| Layered digraph (Sugiyama-like) | Y | Y | Y | Y (dag) | Y | Y |
| Clustered / compound graph | Y | ? | P | N | Y | Y |
| Sequence / swimlane time | P | N | Y | Y (seq) | N | Y |
| Typed boxed design graphs | Y | N | Y | N | P | Y |
| Tree hierarchy | Y | N | Y | Y (tree) | P | Y |
| Math typography | N | N | N | Y | N | N |
| Table grid | N | N | N | Y | N | P |
| Ordered time axis / bars | P (timeline axis) | N | Y | N | N | Y |
| Chart geometry | N | N | Y | N | N | Y |

**llmaid plan:** implement engines behind Mermaid parsers (see ROADMAP engine map).

---

## 11. “Express X as Mermaid” cheat sheet

| Want from diagon / graph-easy | Mermaid expression | llmaid status |
|------------------------------|--------------------|---------------|
| DAG / flowchart | `flowchart` / `graph` | **Y** |
| Edge labels & styles | `-->` `-.->` `==>` \|label\| | **Y** |
| Node shapes | `[ ] ( ) { }` … | **Y** (hints) |
| Groups / machines | `subgraph` | **Y** |
| Sequence | `sequenceDiagram` | **P** (participants, messages, notes, activation, loop/alt/opt) |
| Tree | `mindmap` or flowchart TB | **P** (one plain-label root hierarchy) |
| Class / ER / state | respective types | **P** (core subsets) |
| Timeline | `timeline` | **P** (title, periods/events, sections) |
| Git history | `gitGraph` | **P** (commits, branch lanes, checkout/switch, merges) |
| Table | Markdown table (not Mermaid) | park |
| Math | not Mermaid | park |

---

## 12. Roll-up scorecard (honest, rough)

| Goal | Leader today | llmaid |
|------|--------------|--------|
| Terminal flowchart **quality** (thesis) | **llmaid** | Leading |
| Terminal Mermaid **breadth** | **termaid** | Flowchart, sequence, core design-doc types, mindmap, and timeline slices |
| Non-Mermaid ASCII kit | **diagon** | — |
| General graph language + clusters | **graph-easy** | Missing subgraphs |
| Full language reference | **mermaid.js** | Slice only |
| Agent loop (Mermaid + errors + speed) | **llmaid** (intent) | Strong on slice |

---

## Maintenance

When you ship a feature:

1. Flip **llmaid** cells **N→P→Y** in the right section.
2. Tick the phase item in `ROADMAP.md`.
3. Add/adjust `BEHAVIORS.md` + tests if it’s a user promise.
4. Note non-obvious competitor corrections with a dated note below.

### Changelog of matrix corrections

| Date | Note |
|------|------|
| 2026-07-09 | Initial matrix from design + local tool probes (tw, termaid demos, diagon modes, graph-easy). |
| 2026-07-09 | Phase 0: TB/BT labels, RL goldens, mono-chain straighten; flowchart direction cells strengthened. |
| 2026-07-09 | Phase 1.1: real subgraphs (parse membership, bbox frames, titles). |
| 2026-07-11 | Phase 2 core: participants/actors, implicit participants, lifelines, `->>` messages, and `-->>` returns. |
| 2026-07-11 | Phase 2.3: left/right/over notes and balanced explicit activate/deactivate bars; B18 + focused goldens. |
| 2026-07-11 | Sequence controls: nested loop/alt/else/opt frames; B20 + golden coverage. |
| 2026-07-11 | Agent audit: stable `--audit=json` plus 284 generated directional renders and 71 mirror-signature checks. |
| 2026-07-11 | Phase 4.1: core ordered `mindmap`, reusable integer tree layout, exact parent-child contracts, and four goldens. |
| 2026-07-11 | Phase 4.2: core ordered `timeline`, reusable integer temporal layout, exact spine/attachment/section contracts, 170 generated structures, and five goldens. Local termaid 0.7.1 comparison confirmed its compact rail as a useful baseline but found weaker continuation ownership, section containment, and width semantics. |
| 2026-07-11 | Phase 4.3: core ordered `gitGraph` with stable branch lanes, exact parent-port and merge-channel contracts, 20 generated two-branch schedules, and two focused goldens. |
