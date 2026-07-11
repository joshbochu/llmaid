//! Print grid/symmetry metrics for every golden case.
//!
//! ```sh
//! cargo run -q --example symmetry
//! cargo run -q --example symmetry -- fanout
//! ```
//!
//! Scores come from the exact topology-aware geometry audit. Lower is better
//! for every quality column; `hard` must always be zero. Superscript `²`
//! denotes doubled-cell coordinates, not a squared value.

use llmaid::metrics::{self, format_header, format_row};
use llmaid::parse;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

fn main() {
    let filter = env::args().nth(1);
    let dir = cases_dir();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("tests/cases: {e}");
            process::exit(1);
        })
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".mmd").map(str::to_string)
        })
        .collect();
    names.sort();
    if let Some(ref f) = filter {
        names.retain(|n| n.contains(f.as_str()));
    }
    if names.is_empty() {
        eprintln!("no cases matched");
        process::exit(1);
    }

    println!("{}", format_header());
    println!("{}", "-".repeat(format_header().len()));

    for name in &names {
        let src = fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap_or_else(|e| {
            eprintln!("{name}: {e}");
            process::exit(1);
        });
        let graph = match parse::parse(&src) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{name}: parse error: {e}");
                continue;
            }
        };
        if graph.nodes.is_empty() {
            println!("{name:<20} (empty)");
            continue;
        }
        let m = metrics::measure_graph(&graph, 100);
        println!("{}", format_row(name, &m));
    }

    println!();
    println!("legend: hard=edge/non-endpoint-box violations (must be 0)");
    println!("        rank²/mono²/fork²/merge²=avoidable exact alignment residuals");
    println!("        mirror²/n=eligible diamond residual/count; cross=edge crossing cells");
    println!("        bends=total direction changes; wire=total routed Manhattan length");
}
