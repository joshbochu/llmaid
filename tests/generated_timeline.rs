use llmaid::render;
use llmaid::style::Style;
use llmaid::timeline;

fn structural_source(events: &[usize], cuts: usize) -> String {
    let mut output = String::from("timeline\n");
    let mut section = 0;
    for (period, &event_count) in events.iter().enumerate() {
        if period == 0 || cuts & (1 << (period - 1)) != 0 {
            output.push_str(&format!("  section S{section}\n"));
            section += 1;
        }
        output.push_str(&format!("  P{period} : E{period}-0"));
        for event in 1..event_count {
            output.push_str(&format!(" : E{period}-{event}"));
        }
        output.push('\n');
    }
    output
}

#[test]
fn all_170_small_period_event_and_section_structures_are_clean_ordered_and_deterministic() {
    let mut cases = 0;
    for periods in 1usize..=4 {
        for breadth_mask in 0..1usize << periods {
            let events: Vec<usize> = (0..periods)
                .map(|period| 1 + usize::from(breadth_mask & (1 << period) != 0))
                .collect();
            for cuts in 0..1usize << periods.saturating_sub(1) {
                let source = structural_source(&events, cuts);
                let diagram =
                    timeline::parse(&source).unwrap_or_else(|error| panic!("{error}\n{source}"));
                assert_eq!(diagram.periods.len(), periods, "{source}");
                assert_eq!(diagram.event_count(), events.iter().sum(), "{source}");
                assert_eq!(
                    diagram.sections.len(),
                    1 + cuts.count_ones() as usize,
                    "{source}"
                );
                for (period, value) in diagram.periods.iter().enumerate() {
                    assert_eq!(value.label, format!("P{period}"), "{source}");
                    assert_eq!(value.events.len(), events[period], "{source}");
                }

                let scene = timeline::scene(&diagram, 100);
                assert_eq!(scene, timeline::scene(&diagram, 100), "{source}");
                for ascii in [false, true] {
                    let (first, failures) =
                        render::render_scene_with_checks(&scene, Style { ascii });
                    assert!(
                        failures.is_empty(),
                        "{}\n{source}\n{first}",
                        failures.join("; ")
                    );
                    assert!(!first.contains('…'), "{source}\n{first}");
                    assert_eq!(
                        first,
                        render::render_scene(&scene, Style { ascii }),
                        "{source}"
                    );
                    if ascii {
                        assert!(first.is_ascii(), "{source}\n{first}");
                    }
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 170);
}

#[test]
fn stress_timelines_cover_depth_breadth_unicode_long_labels_and_tight_widths() {
    let cases = [
        "timeline\nP0 : E0\nP1 : E1\nP2 : E2\nP3 : E3\nP4 : E4\nP5 : E5\nP6 : E6\nP7 : E7\n",
        "timeline\nOne : a\nBroad : a : b : c : d : e\nLast : z\n",
        "timeline\ntitle 国際 release 🚀\nsection 基盤\n段階一 : 解析 : δοκιμή\n段階二 : 配布 🚀\n",
        "timeline\ntitle A deliberately long release history title\nsection A long named foundation section\nA period label with several words : An event label with several words : Supercalifragilisticexpialidocious\n",
    ];

    for source in cases {
        let diagram = timeline::parse(source).unwrap();
        for width in [1, 18, 40, 100] {
            let scene = timeline::scene(&diagram, width);
            assert_eq!(
                scene,
                timeline::scene(&diagram, width),
                "width={width}\n{source}"
            );
            for ascii in [false, true] {
                let (rendered, failures) =
                    render::render_scene_with_checks(&scene, Style { ascii });
                assert!(
                    failures.is_empty(),
                    "{}\nwidth={width}\n{source}\n{rendered}",
                    failures.join("; ")
                );
                assert!(!rendered.contains('…'), "{source}\n{rendered}");
                for label in diagram.labels() {
                    let wanted: String = label.chars().filter(|ch| ch.is_alphanumeric()).collect();
                    assert!(
                        alnum_subsequence(&rendered, &wanted),
                        "missing {label:?}\n{rendered}"
                    );
                }
                if ascii {
                    for drawing in ['╭', '╮', '╰', '╯', '─', '│', '├', '┤', '┬', '┴', '┼']
                    {
                        assert!(!rendered.contains(drawing), "width={width}\n{rendered}");
                    }
                }
            }
        }
    }
}

#[test]
fn same_width_labels_and_titles_leave_relative_temporal_geometry_unchanged() {
    let a = timeline::parse("timeline\nsection Alpha\nQ1 : Build\nQ2 : Ship!\n").unwrap();
    let b = timeline::parse("timeline\nsection Bravo\nX1 : Parse\nX2 : Send!\n").unwrap();
    let a = timeline::scene(&a, 100);
    let b = timeline::scene(&b, 100);
    assert_eq!(
        a.groups.iter().map(|group| group.rect).collect::<Vec<_>>(),
        b.groups.iter().map(|group| group.rect).collect::<Vec<_>>()
    );
    assert_eq!(a.paths, b.paths);
    assert_eq!(a.edges, b.edges);
}

fn alnum_subsequence(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars().filter(|ch| ch.is_alphanumeric());
    for wanted in needle.chars() {
        loop {
            match chars.next() {
                Some(got) if got == wanted => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}
