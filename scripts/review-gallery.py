#!/usr/bin/env python3
"""Browser and terminal golden review with persistent visual annotations."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import unicodedata
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import NamedTuple, Optional, Sequence


ROOT = Path(__file__).resolve().parents[1]
CASES_DIR = ROOT / "tests" / "cases"
DEFAULT_NOTES = ROOT / ".llmaid-review.json"
STATUSES = ("unreviewed", "pass", "needs-work")


class Case(NamedTuple):
    name: str
    source: Path
    golden: Path


def discover_cases(cases_dir: Path, filter_text: Optional[str] = None) -> list[Case]:
    needle = filter_text.casefold() if filter_text else None
    cases = []
    for source in sorted(cases_dir.glob("*.mmd")):
        name = source.stem
        golden = source.with_suffix(".txt")
        if not golden.is_file() or (needle and needle not in name.casefold()):
            continue
        cases.append(Case(name=name, source=source, golden=golden))
    return cases


class ReviewStore:
    def __init__(self, path: Path):
        self.path = path
        self.data = {"version": 1, "cases": {}}
        if path.is_file():
            loaded = json.loads(path.read_text(encoding="utf-8"))
            if loaded.get("version") != 1 or not isinstance(loaded.get("cases"), dict):
                raise ValueError(f"unsupported review file: {path}")
            self.data = loaded

    def record(self, name: str) -> dict:
        return self.data["cases"].setdefault(
            name,
            {"status": "unreviewed", "notes": []},
        )

    def set_status(self, name: str, status: str) -> None:
        if status not in STATUSES:
            raise ValueError(f"unknown status: {status}")
        self.record(name)["status"] = status

    def add_note(self, name: str, note: str) -> None:
        note = note.strip()
        if note:
            self.record(name)["notes"].append(note)

    def delete_last_note(self, name: str) -> bool:
        notes = self.record(name)["notes"]
        if not notes:
            return False
        notes.pop()
        return True

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temporary = self.path.with_name(f".{self.path.name}.tmp")
        temporary.write_text(
            json.dumps(self.data, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        os.replace(temporary, self.path)


def validate_review_payload(payload: object, allowed_cases: set[str]) -> dict:
    if not isinstance(payload, dict) or payload.get("version") != 1:
        raise ValueError("review payload must have version 1")
    records = payload.get("cases")
    if not isinstance(records, dict):
        raise ValueError("review payload cases must be an object")
    normalized = {"version": 1, "cases": {}}
    for name, record in records.items():
        if name not in allowed_cases:
            raise ValueError(f"unknown review case: {name}")
        if not isinstance(record, dict):
            raise ValueError(f"review case `{name}` must be an object")
        status = record.get("status", "unreviewed")
        notes = record.get("notes", [])
        if status not in STATUSES:
            raise ValueError(f"invalid status for `{name}`: {status}")
        if not isinstance(notes, list) or not all(isinstance(note, str) for note in notes):
            raise ValueError(f"notes for `{name}` must be strings")
        if len(notes) > 200 or any(len(note) > 10_000 for note in notes):
            raise ValueError(f"notes for `{name}` exceed review limits")
        normalized["cases"][name] = {"status": status, "notes": notes}
    return normalized


def load_diagram(case: Case, live: bool = False) -> str:
    if not live:
        return case.golden.read_text(encoding="utf-8")
    completed = subprocess.run(
        ["cargo", "run", "-q", "--", str(case.source)],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or f"render failed: {case.name}")
    return completed.stdout


def terminal_cell_lines(diagram: str) -> list[list[list[object]]]:
    """Split text into terminal cells, preserving wide and combining glyphs."""
    lines: list[list[list[object]]] = []
    for raw_line in diagram.splitlines():
        cells: list[list[object]] = []
        for character in raw_line:
            category = unicodedata.category(character)
            if unicodedata.combining(character) or category in ("Mn", "Me", "Cf"):
                if cells:
                    cells[-1][0] = str(cells[-1][0]) + character
                continue
            width = 2 if unicodedata.east_asian_width(character) in ("W", "F") else 1
            cells.append([character, width])
        lines.append(cells)
    return lines


def format_slide(
    case: Case,
    diagram: str,
    record: dict,
    index: int,
    total: int,
    *,
    plain: bool,
) -> str:
    title = f"{case.name}  [{index + 1}/{total}]"
    status = record.get("status", "unreviewed")
    notes = record.get("notes", [])
    if plain:
        header = f"{title}\nstatus: {status}\n{'─' * max(24, len(title))}\n"
    else:
        header = f"\033[1m{title}\033[0m\nstatus: {status}\n{'─' * max(24, len(title))}\n"
    body = diagram if diagram.endswith("\n") else diagram + "\n"
    annotation = ""
    if notes:
        annotation = "\nannotations\n" + "".join(f"  • {note}\n" for note in notes)
    return header + body + annotation


REVIEW_HTML_TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>llmaid golden review</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    * { box-sizing: border-box; }
    body { margin: 0; background: Canvas; color: CanvasText; }
    button, select, textarea, input { font: inherit; }
    button, select, textarea, .import-label {
      border: 1px solid color-mix(in srgb, CanvasText 28%, Canvas);
      border-radius: 6px;
      background: Canvas;
      color: CanvasText;
    }
    button, .import-label { padding: 0.45rem 0.7rem; cursor: pointer; }
    button:hover, .import-label:hover { background: color-mix(in srgb, CanvasText 8%, Canvas); }
    button[aria-current="true"] { outline: 2px solid Highlight; outline-offset: -2px; }
    button:focus-visible, select:focus-visible, textarea:focus-visible, .import-label:focus-within {
      outline: 2px solid Highlight;
      outline-offset: 2px;
    }
    .app { display: grid; grid-template-columns: minmax(180px, 240px) minmax(0, 1fr); min-height: 100vh; }
    .sidebar { border-right: 1px solid color-mix(in srgb, CanvasText 20%, Canvas); padding: 1rem; }
    .sidebar h1 { margin: 0 0 0.35rem; font-size: 1.05rem; font-weight: 600; }
    .summary { margin: 0 0 1rem; color: color-mix(in srgb, CanvasText 68%, Canvas); font-size: 0.9rem; }
    .case-list { display: grid; gap: 0.35rem; }
    .case-button { display: flex; justify-content: space-between; width: 100%; text-align: left; }
    .status-symbol { font-family: ui-monospace, monospace; }
    .main { min-width: 0; padding: 1rem 1.25rem 2rem; }
    .toolbar, .case-toolbar, .review-actions { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; }
    .toolbar { justify-content: space-between; margin-bottom: 1rem; }
    .toolbar-group { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; }
    .save-state { color: color-mix(in srgb, CanvasText 68%, Canvas); font-size: 0.9rem; }
    .case-toolbar { margin-bottom: 0.75rem; }
    .case-toolbar select { min-width: 12rem; padding: 0.45rem; }
    .diagram-wrap {
      border-block: 1px solid color-mix(in srgb, CanvasText 18%, Canvas);
      margin-bottom: 1rem;
      overflow: auto;
      padding: 1.25rem 0;
    }
    pre {
      margin: 0;
      min-width: max-content;
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
      font-size: 14px;
      line-height: 1.2;
      font-variant-ligatures: none;
      white-space: pre;
    }
    .terminal-row { display: block; min-height: 1.2em; }
    .terminal-cell { display: inline-block; text-align: center; vertical-align: top; }
    .review-grid { display: grid; grid-template-columns: minmax(12rem, 0.35fr) minmax(18rem, 1fr); gap: 1rem; }
    label { display: grid; gap: 0.35rem; font-weight: 600; }
    label span { font-size: 0.9rem; }
    select, textarea { width: 100%; padding: 0.55rem; }
    textarea { min-height: 8rem; resize: vertical; font-family: ui-monospace, monospace; font-weight: 400; }
    .review-actions { margin-top: 0.75rem; }
    .primary { background: Highlight; color: HighlightText; border-color: Highlight; }
    .primary:hover { background: Highlight; }
    .import-label input { position: absolute; inline-size: 1px; block-size: 1px; opacity: 0; }
    .help { margin: 0.75rem 0 0; color: color-mix(in srgb, CanvasText 68%, Canvas); font-size: 0.85rem; }
    @media (max-width: 760px) {
      .app { grid-template-columns: 1fr; }
      .sidebar { border-right: 0; border-bottom: 1px solid color-mix(in srgb, CanvasText 20%, Canvas); }
      .case-list { grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); }
      .review-grid { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <div class="app">
    <aside class="sidebar">
      <h1>llmaid golden review</h1>
      <p class="summary" id="summary"></p>
      <nav class="case-list" id="case-list" aria-label="Golden cases"></nav>
    </aside>
    <main class="main">
      <div class="toolbar">
        <div class="toolbar-group">
          <button type="button" id="export-json">Export JSON</button>
          <button type="button" id="copy-json">Copy JSON</button>
          <label class="import-label">Import JSON<input type="file" id="import-json" accept="application/json,.json"></label>
        </div>
        <span class="save-state" id="save-state" aria-live="polite"></span>
      </div>
      <div class="case-toolbar">
        <button type="button" id="previous">Previous</button>
        <select id="case-select" aria-label="Current case"></select>
        <button type="button" id="next">Next</button>
      </div>
      <div class="diagram-wrap">
        <pre id="diagram" aria-label="Rendered terminal diagram"></pre>
      </div>
      <div class="review-grid">
        <label><span>Status</span>
          <select id="review-status">
            <option value="unreviewed">Unreviewed</option>
            <option value="pass">Pass</option>
            <option value="needs-work">Needs work</option>
          </select>
        </label>
        <label><span>Annotations — one item per line</span>
          <textarea id="review-note" placeholder="Alignment, spacing, routing, composition…"></textarea>
        </label>
      </div>
      <div class="review-actions">
        <button type="button" id="pass-next" class="primary">Pass &amp; next</button>
        <button type="button" id="flag-next">Needs work &amp; next</button>
      </div>
      <p class="help">Arrow keys change cases when focus is outside a field. Served mode saves directly to .llmaid-review.json; static mode uses browser storage and Export JSON.</p>
    </main>
  </div>
  <script id="bootstrap" type="application/json">__DATA__</script>
  <script>
    (() => {
      const bootstrap = JSON.parse(document.getElementById("bootstrap").textContent);
      const items = bootstrap.items;
      const allowed = new Set(items.map(item => item.name));
      const apiEnabled = __API__;
      const storageKey = "llmaid-review-v1";
      let state = normalize(bootstrap.review);
      let selected = Math.max(0, Math.min(Number(localStorage.getItem(storageKey + "-selected") || 0), items.length - 1));
      let saveTimer;

      const caseList = document.getElementById("case-list");
      const caseSelect = document.getElementById("case-select");
      const diagram = document.getElementById("diagram");
      const reviewStatus = document.getElementById("review-status");
      const reviewNote = document.getElementById("review-note");
      const summary = document.getElementById("summary");
      const saveState = document.getElementById("save-state");

      function normalize(candidate) {
        const output = {version: 1, cases: {}};
        if (!candidate || candidate.version !== 1 || typeof candidate.cases !== "object") return output;
        Object.entries(candidate.cases).forEach(([name, record]) => {
          if (!allowed.has(name) || !record || !["unreviewed", "pass", "needs-work"].includes(record.status)) return;
          const notes = Array.isArray(record.notes) ? record.notes.filter(note => typeof note === "string") : [];
          output.cases[name] = {status: record.status, notes};
        });
        return output;
      }

      function record(name) {
        if (!state.cases[name]) state.cases[name] = {status: "unreviewed", notes: []};
        return state.cases[name];
      }

      function statusSymbol(status) {
        return status === "pass" ? "✓" : status === "needs-work" ? "!" : "○";
      }

      function renderTerminalCells(item) {
        const fragment = document.createDocumentFragment();
        item.cells.forEach(row => {
          const line = document.createElement("span");
          line.className = "terminal-row";
          row.forEach(([text, width]) => {
            const cell = document.createElement("span");
            cell.className = "terminal-cell";
            cell.style.width = `${width}ch`;
            cell.textContent = text;
            line.appendChild(cell);
          });
          fragment.appendChild(line);
        });
        diagram.replaceChildren(fragment);
      }

      function render() {
        const item = items[selected];
        const current = record(item.name);
        renderTerminalCells(item);
        diagram.setAttribute("aria-label", `${item.name} rendered terminal diagram`);
        caseSelect.value = String(selected);
        reviewStatus.value = current.status;
        reviewNote.value = current.notes.join("\n");
        caseList.replaceChildren();
        items.forEach((entry, index) => {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "case-button";
          button.dataset.index = String(index);
          button.setAttribute("aria-current", String(index === selected));
          const name = document.createElement("span");
          name.textContent = entry.name;
          const symbol = document.createElement("span");
          symbol.className = "status-symbol";
          symbol.textContent = statusSymbol(record(entry.name).status);
          button.append(name, symbol);
          button.addEventListener("click", () => select(index));
          caseList.appendChild(button);
        });
        const reviewed = items.filter(item => record(item.name).status !== "unreviewed").length;
        const flagged = items.filter(item => record(item.name).status === "needs-work").length;
        summary.textContent = `${reviewed}/${items.length} reviewed · ${flagged} need work`;
      }

      function select(index) {
        selected = (index + items.length) % items.length;
        localStorage.setItem(storageKey + "-selected", String(selected));
        render();
      }

      function persist() {
        localStorage.setItem(storageKey, JSON.stringify(state));
        saveState.textContent = apiEnabled ? "Saving…" : "Saved in browser";
        if (!apiEnabled) return;
        clearTimeout(saveTimer);
        saveTimer = setTimeout(async () => {
          try {
            const response = await fetch("/api/review", {
              method: "PUT",
              headers: {"Content-Type": "application/json"},
              body: JSON.stringify(state),
            });
            if (!response.ok) throw new Error(`save failed: ${response.status}`);
            saveState.textContent = "Saved to .llmaid-review.json";
          } catch (error) {
            saveState.textContent = error.message;
          }
        }, 200);
      }

      function updateStatus(status, advance) {
        record(items[selected].name).status = status;
        persist();
        if (advance) select(selected + 1); else render();
      }

      function exportJson() {
        const blob = new Blob([JSON.stringify(state, null, 2) + "\n"], {type: "application/json"});
        const link = document.createElement("a");
        link.href = URL.createObjectURL(blob);
        link.download = "llmaid-review.json";
        link.click();
        URL.revokeObjectURL(link.href);
      }

      async function importJson(file) {
        const parsed = normalize(JSON.parse(await file.text()));
        state = parsed;
        persist();
        render();
      }

      items.forEach((item, index) => {
        const option = document.createElement("option");
        option.value = String(index);
        option.textContent = item.name;
        caseSelect.appendChild(option);
      });

      caseSelect.addEventListener("change", () => select(Number(caseSelect.value)));
      document.getElementById("previous").addEventListener("click", () => select(selected - 1));
      document.getElementById("next").addEventListener("click", () => select(selected + 1));
      reviewStatus.addEventListener("change", () => updateStatus(reviewStatus.value, false));
      reviewNote.addEventListener("input", () => {
        record(items[selected].name).notes = reviewNote.value.split("\n").map(note => note.trim()).filter(Boolean);
        persist();
      });
      document.getElementById("pass-next").addEventListener("click", () => updateStatus("pass", true));
      document.getElementById("flag-next").addEventListener("click", () => updateStatus("needs-work", true));
      document.getElementById("export-json").addEventListener("click", exportJson);
      document.getElementById("copy-json").addEventListener("click", async () => {
        await navigator.clipboard.writeText(JSON.stringify(state, null, 2) + "\n");
        saveState.textContent = "Copied review JSON";
      });
      document.getElementById("import-json").addEventListener("change", event => {
        const file = event.target.files[0];
        if (file) importJson(file).catch(error => { saveState.textContent = error.message; });
        event.target.value = "";
      });
      window.addEventListener("keydown", event => {
        if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement.tagName)) return;
        if (event.key === "ArrowLeft") select(selected - 1);
        if (event.key === "ArrowRight") select(selected + 1);
      });

      try {
        const cached = localStorage.getItem(storageKey);
        if (cached) state = normalize(JSON.parse(cached));
      } catch (_) {}
      render();

      if (apiEnabled) {
        fetch("/api/review")
          .then(response => response.json())
          .then(serverState => { state = normalize(serverState); localStorage.setItem(storageKey, JSON.stringify(state)); render(); saveState.textContent = "Saved to .llmaid-review.json"; })
          .catch(error => { saveState.textContent = error.message; });
      } else {
        saveState.textContent = "Saved in browser";
      }
    })();
  </script>
</body>
</html>
"""


def build_review_html(
    cases: Sequence[Case],
    diagrams: dict[str, str],
    review_data: dict,
    *,
    api_enabled: bool,
) -> str:
    bootstrap = {
        "items": [
            {
                "name": case.name,
                "diagram": diagrams[case.name],
                "cells": terminal_cell_lines(diagrams[case.name]),
            }
            for case in cases
        ],
        "review": validate_review_payload(review_data, {case.name for case in cases}),
    }
    encoded = json.dumps(bootstrap, ensure_ascii=False).replace("</", "<\\/")
    return REVIEW_HTML_TEMPLATE.replace("__DATA__", encoded).replace(
        "__API__",
        "true" if api_enabled else "false",
    )


def make_review_server(
    cases: Sequence[Case],
    diagrams: dict[str, str],
    store: ReviewStore,
    *,
    host: str,
    port: int,
) -> ThreadingHTTPServer:
    allowed = {case.name for case in cases}
    page = build_review_html(cases, diagrams, store.data, api_enabled=True).encode("utf-8")
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def send_bytes(self, status: int, content_type: str, body: bytes) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self) -> None:
            if self.path in ("/", "/index.html"):
                self.send_bytes(200, "text/html; charset=utf-8", page)
            elif self.path == "/api/review":
                with lock:
                    body = (json.dumps(store.data, ensure_ascii=False) + "\n").encode("utf-8")
                self.send_bytes(200, "application/json; charset=utf-8", body)
            else:
                self.send_bytes(404, "text/plain; charset=utf-8", b"not found\n")

        def do_PUT(self) -> None:
            if self.path != "/api/review":
                self.send_bytes(404, "text/plain; charset=utf-8", b"not found\n")
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if length <= 0 or length > 1_000_000:
                    raise ValueError("review payload size is invalid")
                payload = validate_review_payload(
                    json.loads(self.rfile.read(length).decode("utf-8")),
                    allowed,
                )
                with lock:
                    store.data = payload
                    store.save()
                self.send_bytes(200, "application/json; charset=utf-8", b'{"ok":true}\n')
            except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
                self.send_bytes(400, "text/plain; charset=utf-8", (str(error) + "\n").encode("utf-8"))

        def log_message(self, format: str, *args: object) -> None:
            return

    return ThreadingHTTPServer((host, port), Handler)


def find_case(cases: Sequence[Case], requested: str) -> int:
    exact = [index for index, case in enumerate(cases) if case.name == requested]
    if exact:
        return exact[0]
    matches = [index for index, case in enumerate(cases) if requested.casefold() in case.name.casefold()]
    if len(matches) != 1:
        names = ", ".join(case.name for case in cases)
        raise ValueError(f"case `{requested}` is not a unique match; available: {names}")
    return matches[0]


def show_help() -> None:
    print(
        "commands: Enter/n next · p previous · a NOTE annotate · "
        "f needs-work · x pass · u unreviewed · d delete last note · q quit"
    )


def review_loop(
    cases: Sequence[Case],
    store: ReviewStore,
    *,
    start: int,
    live: bool,
    plain: bool,
) -> None:
    index = start
    use_clear = not plain and sys.stdout.isatty()
    while True:
        case = cases[index]
        if use_clear:
            print("\033[2J\033[H", end="")
        print(
            format_slide(
                case,
                load_diagram(case, live),
                store.record(case.name),
                index,
                len(cases),
                plain=plain,
            ),
            end="",
        )
        show_help()
        try:
            command = input("review> ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return
        verb, _, value = command.partition(" ")
        if verb in ("", "n"):
            index = (index + 1) % len(cases)
        elif verb == "p":
            index = (index - 1) % len(cases)
        elif verb == "a":
            note = value.strip() or input("note> ").strip()
            store.add_note(case.name, note)
            store.save()
        elif verb == "f":
            store.set_status(case.name, "needs-work")
            store.save()
        elif verb == "x":
            store.set_status(case.name, "pass")
            store.save()
        elif verb == "u":
            store.set_status(case.name, "unreviewed")
            store.save()
        elif verb == "d":
            if store.delete_last_note(case.name):
                store.save()
        elif verb == "q":
            return
        else:
            show_help()
            input("press Enter to continue ")


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Review llmaid golden diagrams in a browser app or real terminal.",
    )
    parser.add_argument("filter", nargs="?", help="case-name substring")
    parser.add_argument("--case", help="show one exact or uniquely matching case and exit")
    parser.add_argument("--start", help="start the interactive slideshow at this case")
    parser.add_argument("--live", action="store_true", help="render .mmd inputs instead of reading .txt goldens")
    parser.add_argument("--list", action="store_true", help="list matching cases and exit")
    parser.add_argument("--plain", action="store_true", help="disable clear-screen and ANSI styling")
    parser.add_argument("--notes", type=Path, default=DEFAULT_NOTES, help="annotation JSON path")
    parser.add_argument("--status", choices=STATUSES, help="set status; requires --case")
    parser.add_argument("--note", help="append an annotation; requires --case")
    parser.add_argument("--html", type=Path, help="write a standalone all-case browser reviewer")
    parser.add_argument(
        "--serve",
        nargs="?",
        const=8765,
        type=int,
        metavar="PORT",
        help="serve the browser reviewer and auto-save annotations (default port: 8765)",
    )
    parser.add_argument("--bind", default="127.0.0.1", help="review-server bind address")
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    cases = discover_cases(CASES_DIR, args.filter)
    if not cases:
        print("no golden cases matched", file=sys.stderr)
        return 1
    if args.list:
        print("\n".join(case.name for case in cases))
        return 0

    try:
        store = ReviewStore(args.notes)
        if args.html and args.serve is not None:
            raise ValueError("--html and --serve are mutually exclusive")
        if args.html or args.serve is not None:
            diagrams = {case.name: load_diagram(case, args.live) for case in cases}
            if args.html:
                args.html.parent.mkdir(parents=True, exist_ok=True)
                args.html.write_text(
                    build_review_html(cases, diagrams, store.data, api_enabled=False),
                    encoding="utf-8",
                )
                print(args.html.resolve())
                return 0
            server = make_review_server(
                cases,
                diagrams,
                store,
                host=args.bind,
                port=args.serve,
            )
            print(f"llmaid review: http://{args.bind}:{server.server_port}")
            print(f"annotations: {store.path}")
            print("press Ctrl-C to stop")
            try:
                server.serve_forever()
            except KeyboardInterrupt:
                pass
            finally:
                server.server_close()
            return 0
        if args.case:
            index = find_case(cases, args.case)
            case = cases[index]
            if args.status:
                store.set_status(case.name, args.status)
            if args.note:
                store.add_note(case.name, args.note)
            if args.status or args.note:
                store.save()
            print(
                format_slide(
                    case,
                    load_diagram(case, args.live),
                    store.record(case.name),
                    index,
                    len(cases),
                    plain=True,
                ),
                end="",
            )
            return 0
        if args.status or args.note:
            raise ValueError("--status and --note require --case")
        start = find_case(cases, args.start) if args.start else 0
        review_loop(cases, store, start=start, live=args.live, plain=args.plain)
        return 0
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"review-gallery: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
