use llmaid::gitgraph;
use llmaid::render;
use llmaid::style::Style;

#[test]
fn generated_branch_histories_are_deterministic_valid_and_ascii_pure() {
    let histories = [
        "gitGraph\ncommit\ncommit\ncommit\ncommit\ncommit\ncommit\ncommit\ncommit\n",
        "gitGraph\ncommit id: \"root\"\nbranch a\ncommit id: \"a1\"\ncommit id: \"a2\"\ncheckout main\nbranch b\ncommit id: \"b1\"\ncheckout main\nmerge a\nmerge b\n",
        "gitGraph\ncommit id: \"基点\" tag: \"開始\"\nbranch \"feature one\"\ncommit id: \"解析 🚀\" type: HIGHLIGHT\nswitch main\ncommit id: \"release candidate with several words\"\nmerge \"feature one\" id: \"統合\" type: REVERSE\n",
    ];

    for source in histories {
        let graph = gitgraph::parse(source).unwrap();
        for width in [1, 18, 40, 100] {
            let scene = gitgraph::scene(&graph, width);
            assert_eq!(
                scene,
                gitgraph::scene(&graph, width),
                "width={width}\n{source}"
            );
            assert_eq!(scene.foreground_boxes.len(), graph.commits.len());
            assert_eq!(scene.edges.len(), graph.edge_count());
            for ascii in [false, true] {
                let (rendered, failures) =
                    render::render_scene_with_checks(&scene, Style { ascii });
                assert!(
                    failures.is_empty(),
                    "{}\nwidth={width}\n{source}\n{rendered}",
                    failures.join("; ")
                );
                assert!(!rendered.contains('…'), "{source}\n{rendered}");
                if ascii {
                    for drawing in ['╭', '╮', '╰', '╯', '─', '│', '┄', '┬', '┴', '┼']
                    {
                        assert!(!rendered.contains(drawing), "{source}\n{rendered}");
                    }
                }
            }
        }
    }
}

#[test]
fn all_small_two_branch_schedules_preserve_commit_and_parent_order() {
    let mut cases = 0;
    for topic_commits in 1..=4 {
        for main_commits in 0..=4 {
            let mut source = String::from("gitGraph\ncommit id: \"root\"\nbranch topic\n");
            for index in 0..topic_commits {
                source.push_str(&format!("commit id: \"t{index}\"\n"));
            }
            source.push_str("checkout main\n");
            for index in 0..main_commits {
                source.push_str(&format!("commit id: \"m{index}\"\n"));
            }
            source.push_str("merge topic id: \"joined\"\n");

            let graph = gitgraph::parse(&source).unwrap();
            let merge = graph.commits.last().unwrap();
            assert_eq!(merge.parents.len(), 2, "{source}");
            assert_eq!(merge.parents[1], topic_commits, "{source}");
            let expected_main = if main_commits == 0 {
                0
            } else {
                topic_commits + main_commits
            };
            assert_eq!(merge.parents[0], expected_main, "{source}");

            let scene = gitgraph::scene(&graph, 100);
            let (_, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
            assert!(failures.is_empty(), "{}\n{source}", failures.join("; "));
            cases += 1;
        }
    }
    assert_eq!(cases, 20);
}
