import importlib.util
import json
import tempfile
import threading
import unittest
import urllib.request
from pathlib import Path


SCRIPT = Path(__file__).with_name("review-gallery.py")
SPEC = importlib.util.spec_from_file_location("review_gallery", SCRIPT)
review_gallery = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(review_gallery)


class ReviewGalleryTests(unittest.TestCase):
    def test_discovers_sorted_golden_cases_and_filters_by_name(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = Path(directory)
            for name in ["zeta", "alpha", "fanout"]:
                (cases / f"{name}.mmd").write_text(f"flowchart LR\n{name}\n")
                (cases / f"{name}.txt").write_text(f"[{name}]\n")

            discovered = review_gallery.discover_cases(cases)
            filtered = review_gallery.discover_cases(cases, "fan")

            self.assertEqual([case.name for case in discovered], ["alpha", "fanout", "zeta"])
            self.assertEqual([case.name for case in filtered], ["fanout"])

    def test_review_store_round_trips_status_and_notes_atomically(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            store = review_gallery.ReviewStore(path)
            store.set_status("fanout", "needs-work")
            store.add_note("fanout", "Merge is one cell too high")
            store.save()

            payload = json.loads(path.read_text())
            loaded = review_gallery.ReviewStore(path)

            self.assertEqual(payload["version"], 1)
            self.assertEqual(loaded.record("fanout")["status"], "needs-work")
            self.assertEqual(
                loaded.record("fanout")["notes"],
                ["Merge is one cell too high"],
            )

    def test_slide_keeps_diagram_bytes_and_shows_existing_annotations(self):
        case = review_gallery.Case(
            name="fanout",
            source=Path("fanout.mmd"),
            golden=Path("fanout.txt"),
        )
        diagram = "╭───╮\n│ A │\n╰───╯\n"
        record = {"status": "needs-work", "notes": ["Center A"]}

        slide = review_gallery.format_slide(case, diagram, record, 1, 3, plain=True)

        self.assertIn("fanout  [2/3]", slide)
        self.assertIn("status: needs-work", slide)
        self.assertIn("• Center A", slide)
        self.assertIn(diagram, slide)

    def test_builds_standalone_all_case_review_app(self):
        cases = [
            review_gallery.Case("alpha", Path("alpha.mmd"), Path("alpha.txt")),
            review_gallery.Case("fanout", Path("fanout.mmd"), Path("fanout.txt")),
        ]
        diagrams = {"alpha": "[alpha]\n", "fanout": "╭fanout╮\n"}

        html = review_gallery.build_review_html(
            cases,
            diagrams,
            {"version": 1, "cases": {}},
            api_enabled=False,
        )

        self.assertIn("<!doctype html>", html.lower())
        self.assertIn('id="review-note"', html)
        self.assertIn("Export JSON", html)
        self.assertIn("Import JSON", html)
        self.assertIn("alpha", html)
        self.assertIn("fanout", html)
        self.assertIn("╭fanout╮", html)
        self.assertIn("const apiEnabled = false", html)
        self.assertIn("terminal-cell", html)

    def test_terminal_cells_preserve_wide_cjk_and_emoji_columns(self):
        lines = review_gallery.terminal_cell_lines("│ 世界 ├── emoji 🚀 ─▶│\n")
        widths = {text: width for text, width in lines[0] if text.strip()}

        self.assertEqual(widths["世"], 2)
        self.assertEqual(widths["界"], 2)
        self.assertEqual(widths["🚀"], 2)
        self.assertEqual(widths["─"], 1)

    def test_rejects_invalid_or_unknown_review_payload_entries(self):
        valid = {
            "version": 1,
            "cases": {"fanout": {"status": "needs-work", "notes": ["off center"]}},
        }
        self.assertEqual(
            review_gallery.validate_review_payload(valid, {"fanout"}),
            valid,
        )
        with self.assertRaises(ValueError):
            review_gallery.validate_review_payload(
                {"version": 1, "cases": {"fanout": {"status": "broken", "notes": []}}},
                {"fanout"},
            )
        with self.assertRaises(ValueError):
            review_gallery.validate_review_payload(
                {"version": 1, "cases": {"unknown": {"status": "pass", "notes": []}}},
                {"fanout"},
            )

    def test_local_review_server_persists_bulk_annotations(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.json"
            store = review_gallery.ReviewStore(path)
            cases = [review_gallery.Case("fanout", Path("fanout.mmd"), Path("fanout.txt"))]
            server = review_gallery.make_review_server(
                cases,
                {"fanout": "[fanout]\n"},
                store,
                host="127.0.0.1",
                port=0,
            )
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            base = f"http://127.0.0.1:{server.server_port}"
            try:
                with urllib.request.urlopen(base + "/", timeout=2) as response:
                    self.assertIn("fanout", response.read().decode("utf-8"))
                payload = {
                    "version": 1,
                    "cases": {
                        "fanout": {
                            "status": "needs-work",
                            "notes": ["branches feel uneven"],
                        }
                    },
                }
                request = urllib.request.Request(
                    base + "/api/review",
                    data=json.dumps(payload).encode("utf-8"),
                    method="PUT",
                    headers={"Content-Type": "application/json"},
                )
                with urllib.request.urlopen(request, timeout=2) as response:
                    self.assertEqual(response.status, 200)
                self.assertEqual(json.loads(path.read_text()), payload)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main()
