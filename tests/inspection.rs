//! Cross-engine quality gate for the human-reviewed reference gallery.
//!
//! Generated tests protect semantic/render invariants over broad input
//! spaces. This curated corpus additionally promises that every declared
//! layout preference passes; unsupported compositions must be reported as
//! unclassified instead of silently counted as success.

use std::path::PathBuf;

use llmaid::style::Style;
use llmaid::{diagram, inspect, quality};

fn golden_sources() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> =
        std::fs::read_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "mmd"))
            .collect();
    paths.sort();
    paths
}

#[test]
fn simple_inspection_schema_is_byte_stable() {
    let semantic = diagram::parse("flowchart LR\nA --> B\n").unwrap();
    let actual = inspect::json(&semantic, 100, Style { ascii: false });
    assert_eq!(actual, include_str!("fixtures/simple.inspect.json"));
}

#[test]
fn reviewed_gallery_passes_every_applicable_invariant_and_preference() {
    let paths = golden_sources();
    assert!(!paths.is_empty());

    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        let semantic =
            diagram::parse(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let scene = diagram::scene(&semantic, 100);
        let report = quality::evaluate(&semantic, &scene, 100);
        let rendered = llmaid::render::render_scene(&scene, Style { ascii: false });

        assert!(
            !report.has_quality_failures(),
            "{} quality failures: {:?}\n{rendered}",
            path.display(),
            report.quality_failures().collect::<Vec<_>>()
        );

        let first = inspect::json(&semantic, 100, Style { ascii: false });
        let second = inspect::json(&semantic, 100, Style { ascii: false });
        assert_eq!(first, second, "{} inspection changed", path.display());
        assert!(
            first.contains("\"quality_failed_checks\":0"),
            "{}: {first}",
            path.display()
        );
        assert!(first.contains("\"geometry\":{"), "{}", path.display());
        assert!(first.contains("\"canvas\":{"), "{}", path.display());
    }
}
