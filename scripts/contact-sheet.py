#!/usr/bin/env python3
"""Packed contact sheet for golden diagram cases.

Lays every tests/cases/*.txt (or live-rendered) diagram into a shelf-packed
grid: place each cell at its natural size; start a new row when the next
diagram no longer fits the target width.

  ./scripts/contact-sheet.py              # terminal, width = $COLUMNS or 120
  ./scripts/contact-sheet.py --width 100
  ./scripts/contact-sheet.py --html -o gallery.html
  ./scripts/contact-sheet.py --sort area  # denser packing
  ./scripts/contact-sheet.py pipeline     # name substring filter

Run each command on its own line.
"""

from __future__ import annotations

import argparse
import html
import os
import subprocess
import sys
import unicodedata
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CASES = ROOT / "tests" / "cases"


def display_width(s: str) -> int:
    """Terminal columns (East Asian wide/full = 2; rest = 1)."""
    n = 0
    for ch in s:
        if unicodedata.east_asian_width(ch) in ("W", "F"):
            n += 2
        else:
            n += 1
    return n


def pad_line(s: str, width: int, align: str = "left") -> str:
    """Pad/truncate a string to `width` display columns with spaces."""
    w = display_width(s)
    if w > width:
        # Truncate by display width (shouldn't happen for goldens we measure).
        out = []
        used = 0
        for ch in s:
            cw = 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
            if used + cw > width:
                break
            out.append(ch)
            used += cw
        return "".join(out) + " " * (width - used)
    pad = width - w
    if align == "right":
        return " " * pad + s
    if align == "center":
        left = pad // 2
        return " " * left + s + " " * (pad - left)
    return s + " " * pad


def frame_diagram(d: Diagram, cell_w: int, cell_h: int, valign: str) -> list[str]:
    """Fit a diagram into a cell without breaking its internal column grid.

    Lines are left-aligned to the diagram's natural width first, then the whole
    block is centered (or top/bottom-aligned) inside the cell.
    """
    # Natural-width lines (preserve column alignment inside the figure).
    body = [pad_line(line, d.w, "left") for line in d.lines]

    # Horizontal placement of the whole block inside cell_w.
    h_pad = max(cell_w - d.w, 0)
    left = h_pad // 2
    right = h_pad - left
    body = [" " * left + line + " " * right for line in body]

    # Vertical placement inside cell_h.
    v_pad = max(cell_h - d.h, 0)
    if valign == "bottom":
        top = v_pad
    elif valign == "top":
        top = 0
    else:
        top = v_pad // 2
    bottom = v_pad - top
    empty = " " * cell_w
    return [empty] * top + body + [empty] * bottom


@dataclass
class Diagram:
    name: str
    lines: list[str]
    w: int
    h: int

    @classmethod
    def from_text(cls, name: str, text: str) -> Diagram:
        # Strip a single trailing newline; keep internal blank lines.
        if text.endswith("\n"):
            text = text[:-1]
        lines = text.split("\n") if text else []
        w = max((display_width(l) for l in lines), default=0)
        return cls(name=name, lines=lines, w=w, h=len(lines))


def resolve_bin() -> Path:
    release = ROOT / "target" / "release" / "llmaid"
    debug = ROOT / "target" / "debug" / "llmaid"
    if release.is_file() and os.access(release, os.X_OK):
        return release
    if debug.is_file() and os.access(debug, os.X_OK):
        return debug
    subprocess.run(["cargo", "build", "-q"], cwd=ROOT, check=True)
    return debug


def load_diagrams(mode: str, filter_sub: str | None) -> list[Diagram]:
    names = sorted(p.stem for p in CASES.glob("*.mmd"))
    if filter_sub:
        names = [n for n in names if filter_sub in n]
    if not names:
        sys.exit(f"no cases matched (filter={filter_sub!r})")

    out: list[Diagram] = []
    llmaid: Path | None = None
    if mode == "live":
        llmaid = resolve_bin()

    for name in names:
        if mode == "txt":
            path = CASES / f"{name}.txt"
            if not path.is_file():
                print(f"warning: missing {path.name}", file=sys.stderr)
                continue
            text = path.read_text()
        else:
            assert llmaid is not None
            r = subprocess.run(
                [str(llmaid), str(CASES / f"{name}.mmd")],
                capture_output=True,
                text=True,
            )
            if r.returncode != 0:
                print(f"warning: render failed for {name}: {r.stderr}", file=sys.stderr)
                continue
            text = r.stdout
        out.append(Diagram.from_text(name, text))
    return out


def sort_diagrams(diagrams: list[Diagram], key: str) -> list[Diagram]:
    if key == "name":
        return sorted(diagrams, key=lambda d: d.name)
    if key == "height":
        return sorted(diagrams, key=lambda d: (-d.h, d.name))
    if key == "width":
        return sorted(diagrams, key=lambda d: (-d.w, d.name))
    if key == "area":
        return sorted(diagrams, key=lambda d: (-(d.w * d.h), d.name))
    if key == "aspect":
        # Wide first, then tall — keeps LR chains on shared shelves.
        return sorted(
            diagrams,
            key=lambda d: (
                0 if d.w >= d.h * 2 else 1 if d.h >= d.w * 2 else 2,
                -(d.w / d.h if d.h else 0),
                d.name,
            ),
        )
    raise SystemExit(f"unknown sort: {key}")


@dataclass
class Cell:
    diagram: Diagram
    # Content box (diagram only); title sits above in the rendered shelf.
    cell_w: int
    cell_h: int


def shelf_pack(diagrams: list[Diagram], max_width: int, gutter: int) -> list[list[Cell]]:
    """First-fit decreasing shelves: place left-to-right until width exceeded."""
    shelves: list[list[Cell]] = []
    cur: list[Cell] = []
    cur_w = 0

    for d in diagrams:
        # Title band uses the name length; cell width is max(diagram, title).
        title_w = display_width(d.name)
        cell_w = max(d.w, title_w)
        add = cell_w if not cur else gutter + cell_w
        if cur and cur_w + add > max_width:
            shelves.append(cur)
            cur = []
            cur_w = 0
            add = cell_w
        cur.append(Cell(diagram=d, cell_w=cell_w, cell_h=d.h))
        cur_w += add
    if cur:
        shelves.append(cur)
    return shelves


def normalize_shelf_heights(shelves: list[list[Cell]]) -> None:
    """Within each shelf, pad cells to the tallest diagram height."""
    for shelf in shelves:
        if not shelf:
            continue
        h = max(c.cell_h for c in shelf)
        for c in shelf:
            c.cell_h = h


def render_shelf_terminal(shelf: list[Cell], gutter: int, align: str) -> list[str]:
    """Render one shelf: title row, then diagram rows (v-centered in cell_h)."""
    if not shelf:
        return []

    gap = " " * gutter
    titles = gap.join(pad_line(c.diagram.name, c.cell_w, "left") for c in shelf)
    rules = gap.join("─" * c.cell_w for c in shelf)
    out = [titles, rules]

    bodies = [frame_diagram(c.diagram, c.cell_w, c.cell_h, align) for c in shelf]
    max_h = max(len(b) for b in bodies)
    for y in range(max_h):
        out.append(gap.join(bodies[i][y] for i in range(len(shelf))))
    return out


def render_terminal(
    shelves: list[list[Cell]], gutter: int, align: str, max_width: int
) -> str:
    lines: list[str] = []
    header = f"contact sheet  ·  {sum(len(s) for s in shelves)} diagrams  ·  width {max_width}  ·  {len(shelves)} shelves"
    lines.append(header)
    lines.append("═" * min(max_width, max(display_width(header), 40)))
    for i, shelf in enumerate(shelves):
        if i:
            lines.append("")  # shelf separator
        lines.extend(render_shelf_terminal(shelf, gutter, align))
    lines.append("")
    lines.append("─" * min(max_width, 40))
    lines.append("source: tests/cases/*.txt  ·  re-pack: scripts/contact-sheet.py")
    return "\n".join(lines) + "\n"


def render_html(shelves: list[list[Cell]], title: str) -> str:
    """CSS multi-column feel via flex shelves; each card is natural size."""
    cards = []
    for shelf in shelves:
        row_cards = []
        for c in shelf:
            body = html.escape("\n".join(c.diagram.lines))
            row_cards.append(
                f'<figure class="card">'
                f"<figcaption>{html.escape(c.diagram.name)}</figcaption>"
                f'<pre class="diagram">{body}</pre>'
                f"</figure>"
            )
        cards.append(f'<section class="shelf">{"".join(row_cards)}</section>')

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{html.escape(title)}</title>
<style>
  :root {{
    --bg: #0f1115;
    --panel: #171a21;
    --ink: #e6e8ee;
    --muted: #8b93a7;
    --rule: #2a3142;
    --gutter: 1.25rem;
  }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0;
    padding: 1.5rem;
    background: var(--bg);
    color: var(--ink);
    font-family: ui-sans-serif, system-ui, sans-serif;
  }}
  h1 {{
    font-size: 1.1rem;
    font-weight: 600;
    margin: 0 0 0.25rem;
  }}
  .meta {{
    color: var(--muted);
    font-size: 0.85rem;
    margin-bottom: 1.25rem;
  }}
  .shelf {{
    display: flex;
    flex-wrap: wrap;
    gap: var(--gutter);
    align-items: flex-start;
    margin-bottom: var(--gutter);
    padding-bottom: var(--gutter);
    border-bottom: 1px solid var(--rule);
  }}
  .card {{
    margin: 0;
    padding: 0.75rem 0.9rem 0.9rem;
    background: var(--panel);
    border: 1px solid var(--rule);
    border-radius: 8px;
    max-width: 100%;
  }}
  figcaption {{
    font-family: ui-monospace, "JetBrains Mono", "Cascadia Mono", Menlo, monospace;
    font-size: 0.75rem;
    color: var(--muted);
    margin-bottom: 0.5rem;
    letter-spacing: 0.02em;
  }}
  pre.diagram {{
    margin: 0;
    font-family: "JetBrains Mono", "Cascadia Mono", "Sarasa Mono", "Noto Sans Mono CJK",
      ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
    line-height: 1.0;
    letter-spacing: 0;
    font-variant-ligatures: none;
    white-space: pre;
    color: var(--ink);
  }}
</style>
</head>
<body>
  <h1>{html.escape(title)}</h1>
  <p class="meta">Packed contact sheet · golden cases · line-height 1 mono</p>
  {"".join(cards)}
</body>
</html>
"""


def default_width() -> int:
    for key in ("COLUMNS", "CONTACT_SHEET_WIDTH"):
        v = os.environ.get(key)
        if v and v.isdigit() and int(v) >= 40:
            return int(v)
    try:
        import shutil

        w = shutil.get_terminal_size(fallback=(120, 40)).columns
        return max(w, 40)
    except Exception:
        return 120


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("filter", nargs="?", help="substring filter on case name")
    ap.add_argument("--txt", action="store_true", help="use committed *.txt (default)")
    ap.add_argument("--live", action="store_true", help="render with llmaid binary")
    ap.add_argument("--width", type=int, default=None, help="max sheet width in columns (terminal)")
    ap.add_argument("--gutter", type=int, default=3, help="spaces between cells (default 3)")
    ap.add_argument(
        "--sort",
        choices=("name", "height", "width", "area", "aspect"),
        default="name",
        help="diagram order before packing (default: name)",
    )
    ap.add_argument(
        "--align",
        choices=("center", "top", "bottom"),
        default="center",
        help="vertical align of diagram within shelf row (default: center)",
    )
    ap.add_argument("--html", action="store_true", help="emit HTML contact sheet")
    ap.add_argument("-o", "--output", type=Path, help="write to file instead of stdout")
    args = ap.parse_args()

    mode = "live" if args.live else "txt"
    diagrams = load_diagrams(mode, args.filter)
    diagrams = sort_diagrams(diagrams, args.sort)

    width = args.width if args.width is not None else default_width()
    # HTML packing is CSS-driven; still shelf-pack for section grouping at a
    # generous virtual width so HTML shelves roughly match terminal denseness.
    pack_width = width if not args.html else max(width, 160)
    shelves = shelf_pack(diagrams, pack_width, args.gutter)
    normalize_shelf_heights(shelves)

    if args.html:
        text = render_html(shelves, "llmaid golden contact sheet")
    else:
        text = render_terminal(shelves, args.gutter, args.align, width)

    if args.output:
        args.output.write_text(text)
        print(f"wrote {args.output} ({len(diagrams)} diagrams, {len(shelves)} shelves)", file=sys.stderr)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
