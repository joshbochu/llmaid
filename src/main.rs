use std::fmt;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use llmaid::{audit, diagram, inspect, limits, render, style::Style};
use unicode_width::UnicodeWidthChar;

const USAGE: &str = "\
llmaid — Mermaid diagrams rendered for the terminal

Usage: llmaid [OPTIONS] [FILE]
       cat diagram.mmd | llmaid

Reads Mermaid from FILE (or stdin) and writes the diagram or machine report to stdout.

Options:
  --ascii        ASCII structural glyphs; label text is preserved unchanged
  --width <N>    target output width (default: 100; fixed, never terminal-detected,
                 so identical input+flags give identical bytes everywhere)
  --strict       treat warnings (ignored directives, missing header) as errors
  --audit=json   write a stable machine-readable geometry audit instead of a diagram
  --inspect=json write semantic geometry, raster rows, and typed quality checks as JSON
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
    inspect_json: bool,
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
        inspect_json: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--ascii" => opts.ascii = true,
            "--strict" => opts.strict = true,
            "--audit=json" => opts.audit_json = true,
            "--inspect=json" => opts.inspect_json = true,
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
                limits::validate_target_width(n).map_err(|limit| limit.to_string())?;
                opts.width = Some(n);
            }
            "-" => {
                set_input(&mut opts, None)?;
            }
            _ if arg.starts_with("--audit=") => {
                return Err("--audit supports only `json` (use `--audit=json`)".to_string());
            }
            _ if arg.starts_with("--inspect=") => {
                return Err("--inspect supports only `json` (use `--inspect=json`)".to_string());
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
    if opts.audit_json && opts.inspect_json {
        return Err("--audit=json and --inspect=json are mutually exclusive".to_string());
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

#[derive(Debug)]
enum InputError {
    Io(io::Error),
    Limit(limits::ResourceLimit),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Limit(limit) => limit.fmt(f),
        }
    }
}

fn read_input(file: Option<&str>) -> Result<String, InputError> {
    match file {
        Some(path) => read_capped(std::fs::File::open(path).map_err(InputError::Io)?),
        None => read_capped(io::stdin().lock()),
    }
}

/// Read at most one byte beyond the source cap. This uses the same boundary as
/// `diagram::parse`, but avoids `read_to_string` allocating an arbitrary input
/// before the parser has a chance to reject it.
fn read_capped(mut reader: impl Read) -> Result<String, InputError> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8 * 1024];
    let cap_plus_one = limits::MAX_SOURCE_BYTES + 1;
    while bytes.len() < cap_plus_one {
        let remaining = cap_plus_one - bytes.len();
        let read_len = remaining.min(buffer.len());
        let count = reader
            .read(&mut buffer[..read_len])
            .map_err(InputError::Io)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    limits::validate_source_bytes(bytes.len()).map_err(InputError::Limit)?;
    String::from_utf8(bytes)
        .map_err(|error| InputError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))
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
        if !opts.audit_json && !opts.inspect_json {
            return ExitCode::SUCCESS;
        }
    }

    // B8: default width is fixed (no terminal detection) for byte-determinism.
    // B9: overflow ladder lives in layout (compact → wrap → over-width).
    let width = opts.width.unwrap_or(100);

    if opts.audit_json {
        return stdout_exit(write_stdout(&audit::json(&diagram, width)));
    }

    if opts.inspect_json {
        return stdout_exit(write_stdout(&inspect::json(
            &diagram,
            width,
            Style { ascii: opts.ascii },
        )));
    }

    let scene = diagram::scene(&diagram, width);
    match render::render_scene_checked(&scene, Style { ascii: opts.ascii }) {
        Ok(output) => stdout_exit(write_stdout(&output)),
        Err(render::CheckedRenderError::Resource(limit)) => {
            diagnostic(format_args!("llmaid: {limit}"));
            diagnostic(format_args!(
                "llmaid: diagram not written; inspect with `--inspect=json`"
            ));
            ExitCode::from(64)
        }
        Err(render::CheckedRenderError::Invariants(failures)) => {
            for failure in failures {
                diagnostic(format_args!("llmaid: invariant failure: {failure}"));
            }
            diagnostic(format_args!(
                "llmaid: diagram not written; inspect with `--inspect=json`"
            ));
            ExitCode::from(70)
        }
    }
}
