# Handoff — llmaid

Last updated: 2026-07-09 — roadmap + coverage matrix added.

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

**v1 flowchart contracts B1–B14 are landed.** Working tree may still hold
uncommitted **edge-label spacing** (` scan `) polish — commit that as Phase 0.1.

- Pipeline: parse → layout (width ladder) → render (shapes, cycles, parallel ports)
- CLI: `--ascii`, `--width N` (default 100), `--strict`
- Tests: behavior + IR/render goldens + B14 canvas invariants
- Regenerate goldens: `UPDATE_GOLDEN=1 cargo test`

## Next steps (from ROADMAP)

1. **Phase 0** — flowchart polish (commit spacing, RL/BT goldens, TB labels, aesthetics)  
2. **Phase 1** — real subgraphs  
3. **Phase 2** — sequence diagrams  
4. Then design types → planning → selective charts → agent diagnostics → distribute  

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
