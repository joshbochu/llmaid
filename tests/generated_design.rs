//! Small deterministic generated/metamorphic coverage for Phase 3 engines.
//!
//! This intentionally uses a fixed representative corpus, not randomness, so
//! failures are byte-reproducible and the suite stays comfortably under the
//! project's five-second budget.

use llmaid::class;
use llmaid::er;
use llmaid::render;
use llmaid::scene::{Rect, Scene};
use llmaid::state;
use llmaid::style::Style;

const DIRECTIONS: [&str; 4] = ["LR", "RL", "TB", "BT"];

fn assert_scene_contract(scene: &Scene, context: &str, visible: &[&str]) {
    let (unicode, unicode_failures) =
        render::render_scene_with_checks(scene, Style { ascii: false });
    assert!(
        unicode_failures.is_empty(),
        "{}\n{context}\n{unicode}",
        unicode_failures.join("; ")
    );
    assert!(
        !unicode.contains('…'),
        "truncated output\n{context}\n{unicode}"
    );

    let again = render::render_scene(scene, Style { ascii: false });
    assert_eq!(
        unicode, again,
        "non-deterministic Unicode render\n{context}"
    );

    let (ascii, ascii_failures) = render::render_scene_with_checks(scene, Style { ascii: true });
    assert!(
        ascii_failures.is_empty(),
        "{}\n{context}\n{ascii}",
        ascii_failures.join("; ")
    );
    assert!(
        ascii.is_ascii(),
        "non-ASCII glyph in ASCII mode\n{context}\n{ascii}"
    );
    assert!(
        !ascii.contains("..."),
        "ellipsis in ASCII output\n{context}\n{ascii}"
    );

    for expected in visible {
        assert!(
            unicode.contains(expected),
            "missing {expected:?}\n{context}\n{unicode}"
        );
        assert!(
            ascii.contains(expected),
            "ASCII missing {expected:?}\n{context}\n{ascii}"
        );
    }
}

fn assert_opposite_envelopes(lr: Rect, rl: Rect, tb: Rect, bt: Rect, context: &str) {
    assert_eq!(lr, rl, "LR/RL envelope mismatch: {context}");
    // Vertical edge-label bias may contribute a parity column, so only the
    // flow-axis envelope is promised for TB/BT by the shared boxed adapter.
    assert_eq!(tb.h, bt.h, "TB/BT flow envelope mismatch: {context}");
}

fn state_source(direction: &str, body: &str) -> String {
    format!("stateDiagram-v2\ndirection {direction}\n{body}")
}

fn class_source(direction: &str, body: &str) -> String {
    format!("classDiagram\ndirection {direction}\n{body}")
}

fn er_source(direction: &str, body: &str) -> String {
    format!("erDiagram\ndirection {direction}\n{body}")
}

#[test]
fn generated_flat_states_are_clean_deterministic_ascii_and_directional() {
    let cases = [
        ("A --> B: advance\n", &["A", "B", "advance"][..]),
        (
            "Root --> Left: choose left\nRoot --> Right: choose right\n",
            &["Root", "Left", "Right", "choose left", "choose right"][..],
        ),
        (
            "state \"Awaiting input\" as Waiting\nWaiting --> Running: start\nRunning --> Waiting: retry\n",
            &["Awaiting input", "Running", "start", "retry"][..],
        ),
    ];

    for (case_index, (body, visible)) in cases.iter().enumerate() {
        let mut bounds = Vec::new();
        for direction in DIRECTIONS {
            let source = state_source(direction, body);
            let parsed = state::parse(&source).unwrap_or_else(|error| {
                panic!("state case {case_index}/{direction}: {error}\n{source}")
            });
            let first = state::scene(&parsed, 120);
            assert_eq!(first, state::scene(&parsed, 120), "{source}");
            assert_eq!(first.boxes.len(), parsed.states.len(), "{source}");
            assert_eq!(first.edges.len(), parsed.transitions.len(), "{source}");
            assert_scene_contract(&first, &source, visible);
            bounds.push(first.bounds());
        }
        assert_opposite_envelopes(bounds[0], bounds[1], bounds[2], bounds[3], body);
    }
}

#[test]
fn generated_state_markers_are_invariant_clean_and_ascii_pure() {
    for direction in DIRECTIONS {
        let source = state_source(
            direction,
            "[*] --> Ready\nReady --> Finished: complete\nFinished --> [*]\n",
        );
        let parsed = state::parse(&source).unwrap();
        let scene = state::scene(&parsed, 120);
        assert_eq!(scene.boxes.len(), parsed.states.len() + 2, "{source}");
        assert_scene_contract(&scene, &source, &["Ready", "Finished", "complete"]);
    }
}

#[test]
fn generated_classes_are_clean_deterministic_ascii_and_directional() {
    let cases = [
        (
            "class User {\n+String name\n+login() bool\n}\nUser --> Session : creates\n",
            &[
                "User",
                "+String name",
                "+login() bool",
                "Session",
                "creates",
            ][..],
        ),
        (
            "Order \"1\" *-- \"1..*\" LineItem : contains\nLineItem o-- Product : selects\n",
            &[
                "Order",
                "LineItem",
                "Product",
                "1 *-- 1..*",
                "contains",
                "o--",
            ][..],
        ),
        (
            "Client ..> Port : calls\nAdapter ..|> Port : implements\n",
            &["Client", "Port", "Adapter", "..>", "..|>", "implements"][..],
        ),
    ];

    for (case_index, (body, visible)) in cases.iter().enumerate() {
        let mut bounds = Vec::new();
        for direction in DIRECTIONS {
            let source = class_source(direction, body);
            let parsed = class::parse(&source).unwrap_or_else(|error| {
                panic!("class case {case_index}/{direction}: {error}\n{source}")
            });
            let first = class::scene(&parsed, 120);
            assert_eq!(first, class::scene(&parsed, 120), "{source}");
            assert_eq!(first.boxes.len(), parsed.classes.len(), "{source}");
            assert_eq!(first.edges.len(), parsed.relations.len(), "{source}");
            assert_scene_contract(&first, &source, visible);
            bounds.push(first.bounds());
        }
        assert_opposite_envelopes(bounds[0], bounds[1], bounds[2], bounds[3], body);
    }
}

#[test]
fn generated_er_models_are_clean_deterministic_ascii_and_directional() {
    let cases = [
        (
            "CUSTOMER {\nstring id PK\nstring region_id FK\n}\nCUSTOMER ||--o{ ORDER : places\n",
            &[
                "CUSTOMER",
                "string id PK",
                "string region_id FK",
                "ORDER",
                "||--o{",
                "places",
            ][..],
        ),
        (
            "ORDER }o..|| RECEIPT : generates\nRECEIPT ||--|| PAYMENT : settles\n",
            &[
                "ORDER",
                "RECEIPT",
                "PAYMENT",
                "}o..||",
                "generates",
                "settles",
            ][..],
        ),
        (
            "AUTHOR ||--o{ BOOK : writes\nBOOK }o--o{ TAG : tagged\nAUTHOR |o--o| PROFILE : owns\n",
            &[
                "AUTHOR", "BOOK", "TAG", "PROFILE", "writes", "tagged", "owns",
            ][..],
        ),
    ];

    for (case_index, (body, visible)) in cases.iter().enumerate() {
        let mut bounds = Vec::new();
        for direction in DIRECTIONS {
            let source = er_source(direction, body);
            let parsed = er::parse(&source).unwrap_or_else(|error| {
                panic!("ER case {case_index}/{direction}: {error}\n{source}")
            });
            let first = er::scene(&parsed, 120);
            assert_eq!(first, er::scene(&parsed, 120), "{source}");
            assert_eq!(first.boxes.len(), parsed.entities.len(), "{source}");
            assert_eq!(first.edges.len(), parsed.relationships.len(), "{source}");
            assert_scene_contract(&first, &source, visible);
            bounds.push(first.bounds());
        }
        assert_opposite_envelopes(bounds[0], bounds[1], bounds[2], bounds[3], body);
    }
}
