use llmaid::diagram::{self, Diagram};
use llmaid::gitgraph::{self, CommitType, Operation};
use llmaid::render;
use llmaid::style::Style;

#[test]
fn parses_commits_branches_checkouts_and_merges_into_ordered_semantic_ir() {
    let source = "gitGraph\n  commit id: \"root\" tag: \"v1\"\n  branch feature\n  commit id: \"work\" type: HIGHLIGHT\n  checkout main\n  commit id: \"release\"\n  merge feature id: \"merged\" type: REVERSE\n";
    let graph = gitgraph::parse(source).unwrap();

    assert_eq!(graph.branches.len(), 2);
    assert_eq!(graph.branches[0].name, "main");
    assert_eq!(graph.branches[1].name, "feature");
    assert_eq!(graph.commits.len(), 4);
    assert_eq!(graph.commits[0].id, "root");
    assert_eq!(graph.commits[0].tag.as_deref(), Some("v1"));
    assert_eq!(graph.commits[1].branch, 1);
    assert_eq!(graph.commits[1].kind, CommitType::Highlight);
    assert_eq!(graph.commits[3].branch, 0);
    assert_eq!(graph.commits[3].kind, CommitType::Reverse);
    assert_eq!(graph.commits[3].parents, vec![2, 1]);
    assert!(matches!(graph.operations[2], Operation::Commit(1)));
    assert!(matches!(graph.operations[3], Operation::Checkout(0)));
}

#[test]
fn dispatch_switch_alias_empty_graph_and_generated_ids_keep_git_semantics() {
    assert!(matches!(
        diagram::parse("%% comment\n\ngitGraph\ncommit\n").unwrap(),
        Diagram::GitGraph(_)
    ));
    assert!(gitgraph::parse("gitGraph\n").unwrap().is_empty());

    let graph = gitgraph::parse(
        "gitGraph\ncommit\nbranch topic\ncommit\nswitch main\ncommit\nmerge topic\n",
    )
    .unwrap();
    assert_eq!(
        graph
            .commits
            .iter()
            .map(|commit| commit.id.as_str())
            .collect::<Vec<_>>(),
        ["0", "1", "2", "3"]
    );
    assert_eq!(graph.commits[3].parents, vec![2, 1]);
}

#[test]
fn malformed_gitgraph_syntax_names_the_line_and_exact_repair() {
    let cases = [
        ("gitGraph LR\n", 1, "direction is deferred"),
        ("gitGraph\ncommit id: root\n", 2, "quoted string"),
        ("gitGraph\nbranch\n", 2, "branch name"),
        ("gitGraph\ncheckout missing\n", 2, "unknown branch"),
        (
            "gitGraph\nbranch topic\nbranch topic\n",
            3,
            "already exists",
        ),
        (
            "gitGraph\ncommit id: \"same\"\ncommit id: \"same\"\n",
            3,
            "duplicate commit id",
        ),
        (
            "gitGraph\ncommit type: LOUD\n",
            2,
            "NORMAL, REVERSE, or HIGHLIGHT",
        ),
        ("gitGraph\nmerge missing\n", 2, "unknown branch"),
        (
            "gitGraph\nbranch topic\nmerge topic\n",
            3,
            "cannot merge the current branch",
        ),
        (
            "gitGraph\nbranch topic\ncheckout main\nmerge topic\n",
            4,
            "has no commits",
        ),
        (
            "gitGraph\ncherry-pick id: \"0\"\n",
            2,
            "cherry-pick is deferred",
        ),
    ];
    for (source, line, expected) in cases {
        let error = gitgraph::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}: {error}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn scene_is_deterministic_checked_ascii_pure_and_never_truncates() {
    let source = "gitGraph\ncommit id: \"initial release\" tag: \"v1.0\"\nbranch feature\ncommit id: \"unicode-解析\" type: HIGHLIGHT\ncheckout main\nmerge feature id: \"merge feature\"\n";
    let graph = gitgraph::parse(source).unwrap();

    for width in [18, 100] {
        let scene = gitgraph::scene(&graph, width);
        assert_eq!(scene, gitgraph::scene(&graph, width));
        for ascii in [false, true] {
            let (rendered, failures) = render::render_scene_with_checks(&scene, Style { ascii });
            assert!(failures.is_empty(), "{}\n{rendered}", failures.join("; "));
            assert!(!rendered.contains('…'), "{rendered}");
            for label in graph.labels() {
                let wanted: String = label.chars().filter(|ch| ch.is_alphanumeric()).collect();
                assert!(
                    alnum_subsequence(&rendered, &wanted),
                    "missing {label:?}\n{rendered}"
                );
            }
            if ascii {
                for drawing in ['╭', '╮', '╰', '╯', '─', '│', '┄', '┬', '┴', '┼']
                {
                    assert!(!rendered.contains(drawing), "{rendered}");
                }
            }
        }
    }
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
