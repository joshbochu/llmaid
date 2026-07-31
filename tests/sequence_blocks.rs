use llmaid::diagram::{self, Diagram};
use llmaid::render;
use llmaid::sequence::{ControlKind, FragmentKind};
use llmaid::style::Style;

const BLOCKS: &str = "\
sequenceDiagram
  participant Client
  participant API
  loop Retry request
    Client->>API: request
    alt accepted
      API-->>Client: success
    else rejected
      opt retryable
        API-->>Client: retry
      end
    end
  end
";

#[test]
fn parses_nested_control_directives_at_stable_event_boundaries() {
    let Diagram::Sequence(sequence) = diagram::parse(BLOCKS).unwrap() else {
        panic!("expected sequence diagram");
    };
    assert_eq!(sequence.events.len(), 3);
    assert_eq!(sequence.controls.len(), 7);
    assert_eq!(sequence.controls[0].at, 0);
    assert!(matches!(
        &sequence.controls[0].kind,
        ControlKind::Start(FragmentKind::Loop, label) if label == "Retry request"
    ));
    assert_eq!(sequence.controls[1].at, 1);
    assert!(matches!(
        &sequence.controls[1].kind,
        ControlKind::Start(FragmentKind::Alt, label) if label == "accepted"
    ));
    assert_eq!(sequence.controls[2].at, 2);
    assert!(matches!(&sequence.controls[2].kind, ControlKind::Else(label) if label == "rejected"));
    assert_eq!(sequence.controls[3].at, 2);
    assert!(matches!(
        &sequence.controls[3].kind,
        ControlKind::Start(FragmentKind::Opt, label) if label == "retryable"
    ));
    assert!(matches!(sequence.controls[4].kind, ControlKind::End));
    assert!(matches!(sequence.controls[5].kind, ControlKind::End));
    assert!(matches!(sequence.controls[6].kind, ControlKind::End));
}

#[test]
fn malformed_control_blocks_name_the_line_and_expectation() {
    let cases = [
        (
            "sequenceDiagram\nloop retry\nA->>B: call\n",
            2,
            "expected a matching `end`",
        ),
        (
            "sequenceDiagram\nend\n",
            2,
            "expected `loop`, `alt`, or `opt` before `end`",
        ),
        (
            "sequenceDiagram\nloop\nend\n",
            2,
            "expected a label after `loop`",
        ),
        (
            "sequenceDiagram\nopt maybe\nelse no\nend\n",
            3,
            "only valid inside an `alt`",
        ),
        (
            "sequenceDiagram\nalt yes\nelse no\nelse perhaps\nend\n",
            4,
            "only one `else`",
        ),
        (
            "sequenceDiagram\nalt yes\nelse\nend\n",
            3,
            "expected a label after `else`",
        ),
    ];
    for (source, line, expected) in cases {
        let error = diagram::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{error}");
        assert!(error.msg.contains(expected), "got: {error}");
    }
}

#[test]
fn nested_control_blocks_render_closed_frames_without_losing_labels() {
    let parsed = diagram::parse(BLOCKS).unwrap();
    let scene = diagram::scene(&parsed, 100);
    assert_eq!(scene.groups.len(), 3, "loop, one alt frame, and opt");
    let group = |title: &str| {
        scene
            .groups
            .iter()
            .find(|group| group.title.text == title)
            .unwrap_or_else(|| panic!("missing group {title:?}"))
    };
    let alt = group("alt accepted");
    let opt = group("opt retryable");
    assert!(
        scene
            .groups
            .iter()
            .all(|group| !group.title.text.starts_with("else ")),
        "else must subdivide the alt frame rather than create another frame"
    );
    assert_eq!(alt.separators.len(), 1);
    let separator = &alt.separators[0];
    assert_eq!(separator.label.text.trim(), "else rejected");
    assert_eq!(
        separator.label.at,
        llmaid::scene::Point::new(alt.rect.x + 2, separator.y)
    );
    assert!(
        alt.rect
            .contains(llmaid::scene::Point::new(opt.rect.x, opt.rect.y))
            && alt.rect.contains(llmaid::scene::Point::new(
                opt.rect.right() - 1,
                opt.rect.bottom() - 1,
            )),
        "blocks inside else should remain inside the single alt frame: {alt:?} {opt:?}"
    );
    assert!(
        opt.rect.x > alt.rect.x && opt.rect.right() < alt.rect.right(),
        "nested opt needs visible horizontal insets: {alt:?} {opt:?}"
    );
    assert_eq!(
        opt.rect.y,
        separator.y + 2,
        "the branch label owns its divider row and one breathing row"
    );
    let first_lifeline = scene.paths[0].points[0].x;
    let last_lifeline = scene.paths[1].points[0].x;
    for frame in &scene.groups {
        assert!(
            frame.rect.x < first_lifeline && frame.rect.right() > last_lifeline,
            "every fragment frame must contain all participant lifelines: {frame:?}"
        );
    }
    let (unicode, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("\n"));
    for label in [
        "loop Retry request",
        "alt accepted",
        "else rejected",
        "opt retryable",
        "request",
        "success",
        "retry",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    let else_line = unicode
        .lines()
        .find(|line| line.contains("else rejected"))
        .unwrap();
    assert!(
        else_line.contains("├─ else rejected ─") && else_line.contains('┤'),
        "else branch needs a visible full-width separator:\n{unicode}"
    );
    let (ascii, failures) = render::render_scene_with_checks(&scene, Style { ascii: true });
    assert!(failures.is_empty(), "{}\n{ascii}", failures.join("\n"));
    assert!(ascii.is_ascii(), "ASCII mode emitted Unicode:\n{ascii}");
    assert!(
        ascii
            .lines()
            .any(|line| line.contains("+- else rejected -")),
        "ASCII else separator missing:\n{ascii}"
    );
}

#[test]
fn control_block_render_is_deterministic() {
    let parsed = diagram::parse(BLOCKS).unwrap();
    let first = render::render_scene(&diagram::scene(&parsed, 100), Style { ascii: false });
    let second = render::render_scene(&diagram::scene(&parsed, 100), Style { ascii: false });
    assert_eq!(first, second);
}

#[test]
fn activation_immediately_inside_control_stays_inside_its_frame() {
    let source = "\
sequenceDiagram
  participant A
  participant B
  loop work
    activate A
    A->>B: call
    deactivate A
  end
";
    let parsed = diagram::parse(source).unwrap();
    let scene = diagram::scene(&parsed, 100);
    let frame = scene
        .groups
        .iter()
        .find(|group| group.title.text == "loop work")
        .expect("loop frame");
    let activation = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.is_empty())
        .expect("activation bar");
    assert!(
        frame.rect.contains(llmaid::scene::Point::new(
            activation.rect.x,
            activation.rect.y
        )) && frame.rect.contains(llmaid::scene::Point::new(
            activation.rect.right() - 1,
            activation.rect.bottom() - 1,
        )),
        "activation must be contained by loop: {frame:?} {activation:?}"
    );
}

#[test]
fn activation_spanning_alt_separator_remains_closed_and_attached() {
    let source = "\
sequenceDiagram
  participant A
  participant B
  activate A
  alt accepted
    A->>B: call
  else rejected
    B-->>A: retry
  end
  deactivate A
";
    let parsed = diagram::parse(source).unwrap();
    let scene = diagram::scene(&parsed, 100);
    let alt = scene
        .groups
        .iter()
        .find(|group| group.title.text == "alt accepted")
        .expect("alt frame");
    let separator = &alt.separators[0];
    let activation = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.is_empty())
        .expect("activation bar");
    assert!(
        activation.rect.y < separator.y && activation.rect.bottom() > separator.y,
        "activation should span both branches: {activation:?} {separator:?}"
    );

    for ascii in [false, true] {
        let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii });
        assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
        for label in ["alt accepted", "else rejected", "call", "retry"] {
            assert!(output.contains(label), "missing {label:?}:\n{output}");
        }
    }
}
