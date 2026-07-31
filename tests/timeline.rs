use llmaid::diagram::{self, Diagram};
use llmaid::render;
use llmaid::style::Style;
use llmaid::timeline;

#[test]
fn parses_title_sections_periods_and_inline_or_continued_events_in_source_order() {
    let source = "\
timeline
  title Release plan
  section Foundation
    Q1 : Design : Prototype
       : Review
    Q2 : Build
  section Delivery
    Q3 : Ship
";
    let timeline = timeline::parse(source).unwrap();

    assert_eq!(timeline.title.as_deref(), Some("Release plan"));
    assert_eq!(timeline.sections.len(), 2);
    assert_eq!(timeline.sections[0].label, "Foundation");
    assert_eq!(
        (
            timeline.sections[0].first_period,
            timeline.sections[0].period_count
        ),
        (0, 2)
    );
    assert_eq!(
        (
            timeline.sections[1].first_period,
            timeline.sections[1].period_count
        ),
        (2, 1)
    );
    assert_eq!(timeline.periods.len(), 3);
    assert_eq!(timeline.periods[0].label, "Q1");
    assert_eq!(timeline.periods[0].section, Some(0));
    assert_eq!(
        timeline.periods[0]
            .events
            .iter()
            .map(|event| event.label.as_str())
            .collect::<Vec<_>>(),
        ["Design", "Prototype", "Review"]
    );
    assert_eq!(timeline.periods[1].label, "Q2");
    assert_eq!(timeline.periods[2].section, Some(1));
}

#[test]
fn dispatch_empty_and_duplicate_labels_keep_timeline_semantics() {
    assert!(matches!(
        diagram::parse("%% comment\n\ntimeline\nSame : Same\nSame : Same\n").unwrap(),
        Diagram::Timeline(_)
    ));
    assert!(
        timeline::parse("timeline\n%% no periods\n")
            .unwrap()
            .is_empty()
    );

    let parsed = timeline::parse("timeline\nSame : Same\nSame : Same\n").unwrap();
    assert_eq!(parsed.periods.len(), 2);
    assert_eq!(parsed.periods[0].label, "Same");
    assert_eq!(parsed.periods[1].label, "Same");
}

#[test]
fn malformed_timeline_syntax_names_the_line_and_exact_repair() {
    let cases = [
        ("timeline\n  : orphan\n", 2, "period before event"),
        ("timeline\n  Q1 :\n", 2, "non-empty event"),
        ("timeline\n  Q1 :: build\n", 2, "non-empty event"),
        ("timeline\n  Q1 build\n", 2, "period : event"),
        ("timeline\n  section\n", 2, "non-empty section"),
        ("timeline\n  section:\n", 2, "non-empty section"),
        (
            "timeline\n  section Empty\n",
            2,
            "section `Empty` has no period",
        ),
        (
            "timeline\n  section Empty\n  section Next\n  Q1 : build\n",
            2,
            "section `Empty` has no period",
        ),
        (
            "timeline\n  Q1 : build\n  title Late\n",
            3,
            "title before periods",
        ),
        ("timeline\n  title First\n  title Second\n", 3, "only once"),
        ("timeline\n  title: Wrong\n", 2, "without `:`"),
        (
            "timeline\n  Q1 : build\n  section Next\n  : orphan\n",
            4,
            "period before event",
        ),
        ("timeline TD\n  Q1 : build\n", 1, "direction is deferred"),
    ];
    for (source, line, expected) in cases {
        let error = timeline::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}: {error}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn forced_breaks_and_comments_preserve_continuation_order_without_calendar_semantics() {
    let source = "timeline\n  Whenever : First<br>line\n  %% keep the current period\n           : 2026-07-11 is still plain text\n";
    let timeline = timeline::parse(source).unwrap();
    assert_eq!(timeline.periods[0].label, "Whenever");
    assert_eq!(timeline.periods[0].events[0].label, "First\nline");
    assert_eq!(
        timeline.periods[0].events[1].label,
        "2026-07-11 is still plain text"
    );
}

#[test]
fn combining_marks_and_zwj_emoji_render_as_intact_graphemes() {
    for (source, expected) in [
        ("timeline\n  cafe\u{301} : event\n", "cafe\u{301}"),
        ("timeline\n  Q1 : 👨\u{200d}💻\n", "👨\u{200d}💻"),
    ] {
        let timeline = timeline::parse(source).unwrap();
        let scene = timeline::scene(&timeline, 100);
        let (rendered, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
        assert!(failures.is_empty(), "{failures:#?}\n{rendered}");
        assert!(rendered.contains(expected), "{rendered}");
    }
}

#[test]
fn scene_is_deterministic_checked_ascii_pure_and_never_truncates() {
    let source = "timeline\n  title Product history\n  section Build\n  Q1 : Prototype : Review\n  Q2 : Ship\n";
    let timeline = timeline::parse(source).unwrap();
    let scene = timeline::scene(&timeline, 100);
    assert_eq!(scene, timeline::scene(&timeline, 100));
    assert!(scene.boxes.is_empty());
    assert!(scene.texts.len() >= 6);
    assert_eq!(scene.groups.len(), 1);

    for ascii in [false, true] {
        let (rendered, failures) = render::render_scene_with_checks(&scene, Style { ascii });
        assert!(failures.is_empty(), "{}\n{rendered}", failures.join("; "));
        if ascii {
            assert!(rendered.is_ascii(), "{rendered}");
        }
    }
}

#[test]
fn width_pressure_wraps_periods_and_events_before_over_width_fallback() {
    let source = "timeline\n  A period with several words : An event label with several words\n";
    let timeline = timeline::parse(source).unwrap();
    let comfortable = timeline::scene(&timeline, 100);
    let comfortable = render::render_scene(&comfortable, Style { ascii: false });
    assert!(comfortable.contains("A period with several words"));
    assert!(comfortable.contains("An event label with several words"));

    let narrow = timeline::scene(&timeline, 28);
    let rendered = render::render_scene(&narrow, Style { ascii: false });
    assert!(!rendered.contains("A period with several words"));
    assert!(!rendered.contains("An event label with several words"));
    for word in ["period", "several", "words", "event", "label"] {
        assert!(rendered.contains(word), "missing {word:?}\n{rendered}");
    }
}

#[test]
fn width_pressure_preserves_long_identifiers_as_single_tokens() {
    let source =
        "timeline\n  A descriptive period : Prepare release : SupercalifragilisticIdentifier\n";
    let timeline = timeline::parse(source).unwrap();
    let narrow = timeline::scene(&timeline, 20);
    let rendered = render::render_scene(&narrow, Style { ascii: false });

    assert!(
        rendered.contains("SupercalifragilisticIdentifier"),
        "identifier was hard-split:\n{rendered}"
    );
    assert!(
        narrow.bounds().w > 20,
        "intrinsic token should use the readable over-width fallback"
    );
}
