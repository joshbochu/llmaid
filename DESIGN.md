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
- Unicode default, `--ascii` fallback (`+-|>`), `--width N` fit

Explicitly out of v1 (v2 candidates): sequence diagrams, trees/mindmaps,
subgraphs, styling directives (`classDef`, `style` — parsed and ignored, never an error).

## Architecture

```
src/
  main.rs      CLI: args, stdin/file, error reporting
  parse.rs     Mermaid flowchart subset → IR (Graph: nodes, edges, direction)
  layout.rs    IR → integer character-grid positions
  route.rs     Orthogonal edge paths + label placement on the grid
  render.rs    Grid canvas → styled box-drawing output
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

### Renderer

A `Canvas` of `char` cells with draw ops (box, hline, vline, corner, text).
Box-drawing junction resolution (e.g. `─` meeting `│` becomes `┼`) via bitmask
lookup, so crossings and tees always render as the correct glyph.

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
llmaid [FILE]              read Mermaid from FILE or stdin, write to stdout
  --ascii                  pure ASCII charset
  --width <N>              max output width (default: fixed 100 — never
                           terminal-detected, so output is byte-deterministic)
  --strict                 warnings become errors
  --version / --help
```

Exit codes: 0 ok, 1 render error, 64 usage/parse error.

Width overflow ladder (never truncate, never fail): compact inter-node gaps →
wrap labels → render over-width anyway. Labels wrap only under width pressure.
Empty graphs exit 0 (empty stdout, stderr warning). stdout carries only the
diagram; all diagnostics go to stderr.

## Testing

- **Behavior contracts**: `BEHAVIORS.md` (B1–B14) indexes the promised
  behaviors; each has a given/when/then test in `tests/behavior.rs`
  (CLI contracts exercise the real binary).
- **Golden snapshots**: `tests/cases/*.mmd` → `tests/cases/*.txt`, byte-compared.
  Seed set: LR pipeline with edge labels, fork/merge, diamond decision,
  cycle/back-edge, self-loop, CJK + emoji labels, every shape, TB deep chain.
- **Invariant checks** on every rendered frame (also run as fuzz oracle):
  no truncated labels, box borders closed, every edge reaches its endpoints,
  no character overwrites text.
- `cargo test` must stay < 5s.

## Milestones

1. **M1**: parser + IR + golden-test harness (parse-only snapshots)
2. **M2**: layout ranks/ordering; render boxes + straight edges — LR pipeline renders
3. **M3**: elbows, edge labels, fork/merge, TB — the session's reference diagrams render
4. **M4**: shapes, cycles/self-loops, `--ascii`, `--width`, error messages — v1 complete
