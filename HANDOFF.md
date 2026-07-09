# Handoff — llmaid

Last updated: 2026-07-09 — Phase 0 flowchart polish complete.

## What this project is

`llmaid` renders Mermaid as clean, deterministic Unicode diagrams in the
terminal. **Agents create and self-debug; humans look at the visuals.**

Thesis: diagon's alignment correctness + termiflow's aesthetic, behind Mermaid
syntax, in one fast Rust binary — expanding toward multi-type Mermaid breadth
without losing glance quality.

## Read order

1. `AGENTS.md` — commands, modules, invariants, conventions  
2. `DESIGN.md` — v1 design thesis and architecture  
3. `ROADMAP.md` — phased plan (what to build next)  
4. `MATRIX.md` — capability × tool coverage checklist  
5. `BEHAVIORS.md` — shipped contracts B1–B14  
6. `CHANGELOG.md` — decisions D1–D14  

## Current state

**v1 contracts B1–B14 + Phase 0 polish are landed.**

- Pipeline: parse → layout (width ladder, mono-chain straighten) → render
- Edge labels: padded on-shaft (` scan `); TB/BT labels beside vertical runs
- Directions: LR/RL/TB/BT with goldens (`dir-rl`, `dir-bt`, `dir-tb-labels`)
- CLI: `--ascii`, `--width N` (default 100), `--strict`
- Tests: behavior + IR/render goldens + B14 canvas invariants
- Regenerate goldens: `UPDATE_GOLDEN=1 cargo test`

## Next steps (from ROADMAP)

1. **Phase 1** — real subgraphs (graph-easy / architecture parity)  
2. **Phase 2** — sequence diagrams  
3. Design types → planning → charts → agent diagnostics → distribute  

Track detail in `ROADMAP.md`; tick coverage in `MATRIX.md`.

## Quality bar

```
╭────────╮             ╭────────────╮              ╭──────────╮
│ source ├─── scan ───▶│ Vec<Token> ├─── parse ───▶│ Expr AST │
╰────────╯             ╰────────────╯              ╰──────────╯
```

Rounded, labels on the arrow with spaces, nothing truncated. Eyeball
`tests/cases/*.mmd` when unsure.

## Local comparison tools

- termiflow: `tw` (`~/.cargo/bin/tw`)  
- diagon: `node ~/dev/lox-rs/tools/diagon.mjs <seq|tree|math|table|frame|dag|grammar>`  
- graph-easy: `PERL5LIB=~/perl5/lib/perl5 ~/perl5/bin/graph-easy`  
- termaid: `uvx --from termaid termaid`  
