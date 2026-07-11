use std::io::Read;
use std::process::ExitCode;

use llmaid::{diagram, render, style::Style};

const USAGE: &str = "\
llmaid — Mermaid diagrams rendered for the terminal

Usage: llmaid [OPTIONS] [FILE]
       cat diagram.mmd | llmaid

Reads Mermaid from FILE (or stdin) and writes the diagram to stdout.

Options:
  --ascii        pure ASCII output (+, -, |) instead of Unicode box-drawing
  --width <N>    maximum output width (default: 100; fixed, never terminal-detected,
                 so identical input+flags give identical bytes everywhere)
  --strict       treat warnings (ignored directives, missing header) as errors
  -h, --help     print this help
  -V, --version  print version
";

struct Opts {
    file: Option<String>,
    ascii: bool,
    width: Option<usize>,
    strict: bool,
}

fn parse_args() -> Result<Option<Opts>, String> {
    let mut opts = Opts {
        file: None,
        ascii: false,
        width: None,
        strict: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("llmaid {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--ascii" => opts.ascii = true,
            "--strict" => opts.strict = true,
            "--width" => {
                let value = args
                    .next()
                    .ok_or("--width needs a number, e.g. --width 100")?;
                let n: usize = value
                    .parse()
                    .map_err(|_| format!("--width needs a number, got `{value}`"))?;
                opts.width = Some(n);
            }
            "-" => opts.file = None,
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option `{arg}` (see --help)"));
            }
            _ => {
                if opts.file.is_some() {
                    return Err("more than one input file given".to_string());
                }
                opts.file = Some(arg);
            }
        }
    }
    Ok(Some(opts))
}

fn read_input(file: Option<&str>) -> std::io::Result<String> {
    match file {
        Some(path) => std::fs::read_to_string(path),
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(Some(opts)) => opts,
        Ok(None) => return ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("llmaid: {msg}");
            return ExitCode::from(64);
        }
    };

    let src = match read_input(opts.file.as_deref()) {
        Ok(src) => src,
        Err(e) => {
            let name = opts.file.as_deref().unwrap_or("stdin");
            eprintln!("llmaid: cannot read {name}: {e}");
            return ExitCode::from(64);
        }
    };

    let diagram = match diagram::parse(&src) {
        Ok(diagram) => diagram,
        Err(e) => {
            eprintln!("llmaid: {e}");
            return ExitCode::from(64);
        }
    };

    // B6: stdout carries only the diagram; all diagnostics go to stderr.
    for w in diagram.warnings() {
        eprintln!("llmaid: warning: line {}: {}", w.line, w.msg);
    }
    if opts.strict && !diagram.warnings().is_empty() {
        eprintln!("llmaid: failing due to warnings (--strict)");
        return ExitCode::from(64);
    }

    // B7: an empty graph is trivia, not an error — pipelines keep flowing.
    if diagram.is_empty() {
        eprintln!("llmaid: warning: nothing to render (input has no nodes)");
        return ExitCode::SUCCESS;
    }

    // B8: default width is fixed (no terminal detection) for byte-determinism.
    // B9: overflow ladder lives in layout (compact → wrap → over-width).
    let width = opts.width.unwrap_or(100);

    let scene = diagram::scene(&diagram, width);
    let output = render::render_scene(&scene, Style { ascii: opts.ascii });
    print!("{output}");
    ExitCode::SUCCESS
}
