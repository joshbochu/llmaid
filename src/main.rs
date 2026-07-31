use std::fmt;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use llmaid::{audit, diagram, render, style::Style};
use unicode_width::UnicodeWidthChar;

const USAGE: &str = "\
llmaid — Mermaid diagrams rendered for the terminal

Usage: llmaid [OPTIONS] [FILE]
       cat diagram.mmd | llmaid

Reads Mermaid from FILE (or stdin) and writes the diagram or audit to stdout.

Options:
  --ascii        ASCII structural glyphs; label text is preserved unchanged
  --width <N>    target output width (default: 100; fixed, never terminal-detected,
                 so identical input+flags give identical bytes everywhere)
  --strict       treat warnings (ignored directives, missing header) as errors
  --audit=json   write a stable machine-readable geometry audit instead of a diagram
  -h, --help     print this help
  -V, --version  print version
";

struct Opts {
    file: Option<String>,
    input_given: bool,
    ascii: bool,
    width: Option<usize>,
    strict: bool,
    audit_json: bool,
}

enum Action {
    Run(Opts),
    Help,
    Version,
}

fn parse_args() -> Result<Action, String> {
    let mut opts = Opts {
        file: None,
        input_given: false,
        ascii: false,
        width: None,
        strict: false,
        audit_json: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--ascii" => opts.ascii = true,
            "--strict" => opts.strict = true,
            "--audit=json" => opts.audit_json = true,
            "--width" => {
                let value = args
                    .next()
                    .ok_or("--width needs a number, e.g. --width 100")?;
                let n: usize = value.parse().map_err(|_| {
                    format!("--width needs a number, got `{}`", safe_excerpt(&value))
                })?;
                if n == 0 {
                    return Err("--width must be at least 1, got `0`".to_string());
                }
                opts.width = Some(n);
            }
            "-" => {
                set_input(&mut opts, None)?;
            }
            _ if arg.starts_with("--audit=") => {
                return Err("--audit supports only `json` (use `--audit=json`)".to_string());
            }
            _ if arg.starts_with('-') => {
                return Err(format!(
                    "unknown option `{}` (see --help)",
                    safe_excerpt(&arg)
                ));
            }
            _ => {
                set_input(&mut opts, Some(arg))?;
            }
        }
    }
    Ok(Action::Run(opts))
}

fn set_input(opts: &mut Opts, file: Option<String>) -> Result<(), String> {
    if opts.input_given {
        return Err("more than one input source given; use either FILE or `-`".to_string());
    }
    opts.input_given = true;
    opts.file = file;
    Ok(())
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

fn write_stdout(output: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    writer.write_all(output.as_bytes())?;
    writer.flush()
}

fn stdout_exit(result: io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            diagnostic(format_args!("llmaid: cannot write stdout: {error}"));
            ExitCode::from(74)
        }
    }
}

fn diagnostic(args: fmt::Arguments<'_>) {
    let stderr = io::stderr();
    let mut writer = stderr.lock();
    let _ = writeln!(writer, "{args}");
}

fn report_parse_error(source_name: &str, src: &str, error: &llmaid::parse::ParseError) {
    let stderr = io::stderr();
    let mut writer = stderr.lock();
    let _ = writeln!(
        writer,
        "llmaid: {source_name}:{}: error: {}",
        error.line, error.msg
    );
    if let Some(line) = src.lines().nth(error.line.saturating_sub(1)) {
        let excerpt = safe_excerpt(line);
        let _ = writeln!(writer, "  {} | {excerpt}", error.line);
    }
}

fn safe_excerpt(line: &str) -> String {
    let mut excerpt = String::new();
    for ch in line.chars() {
        if ch.is_control() || ch.width() == Some(0) {
            excerpt.extend(ch.escape_default());
        } else {
            excerpt.push(ch);
        }
    }
    excerpt
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(Action::Run(opts)) => opts,
        Ok(Action::Help) => return stdout_exit(write_stdout(USAGE)),
        Ok(Action::Version) => {
            let version = format!("llmaid {}\n", env!("CARGO_PKG_VERSION"));
            return stdout_exit(write_stdout(&version));
        }
        Err(msg) => {
            diagnostic(format_args!("llmaid: {msg}"));
            return ExitCode::from(64);
        }
    };

    let source_name = safe_excerpt(opts.file.as_deref().unwrap_or("<stdin>"));
    let src = match read_input(opts.file.as_deref()) {
        Ok(src) => src,
        Err(e) => {
            diagnostic(format_args!("llmaid: cannot read {source_name}: {e}"));
            return ExitCode::from(64);
        }
    };

    let diagram = match diagram::parse(&src) {
        Ok(diagram) => diagram,
        Err(e) => {
            report_parse_error(&source_name, &src, &e);
            return ExitCode::from(64);
        }
    };

    // B6: stdout carries only the diagram; all diagnostics go to stderr.
    for w in diagram.warnings() {
        diagnostic(format_args!(
            "llmaid: {source_name}:{}: warning: {}",
            w.line, w.msg
        ));
    }
    if opts.strict && !diagram.warnings().is_empty() {
        diagnostic(format_args!("llmaid: failing due to warnings (--strict)"));
        return ExitCode::from(64);
    }

    // B7: an empty graph is trivia, not an error — pipelines keep flowing.
    if diagram.is_empty() {
        diagnostic(format_args!(
            "llmaid: {source_name}: warning: nothing to render (input has no nodes)"
        ));
        if !opts.audit_json {
            return ExitCode::SUCCESS;
        }
    }

    // B8: default width is fixed (no terminal detection) for byte-determinism.
    // B9: overflow ladder lives in layout (compact → wrap → over-width).
    let width = opts.width.unwrap_or(100);

    if opts.audit_json {
        return stdout_exit(write_stdout(&audit::json(&diagram, width)));
    }

    let scene = diagram::scene(&diagram, width);
    match render::render_scene_checked(&scene, Style { ascii: opts.ascii }) {
        Ok(output) => stdout_exit(write_stdout(&output)),
        Err(failures) => {
            for failure in failures {
                diagnostic(format_args!("llmaid: invariant failure: {failure}"));
            }
            diagnostic(format_args!(
                "llmaid: diagram not written; inspect with `--audit=json`"
            ));
            ExitCode::from(70)
        }
    }
}
