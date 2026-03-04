#![forbid(unsafe_code)]

pub mod capsule;
pub mod checks;
pub mod cli;
pub mod csv;
pub mod format;
pub mod normalize;
pub mod orchestrator;
pub mod output;
pub mod profile;
pub mod refusal;
pub mod scan;
pub mod witness;

/// Run the shape pipeline. Returns exit code (0, 1, or 2).
pub fn run() -> Result<u8, Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    let args = match cli::args::Args::parse() {
        Ok(args) => args,
        Err(err) => {
            let exit_code = match err.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => 0,
                _ => 2,
            };
            err.print()?;
            return Ok(exit_code);
        }
    };

    if args.schema {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "shape.v0",
            "description": "JSON output schema for shape structural comparability gate",
            "type": "object",
            "required": ["version", "outcome", "files", "dialect"],
            "properties": {
                "version": { "type": "string", "const": "shape.v0" },
                "outcome": { "type": "string", "enum": ["COMPATIBLE", "INCOMPATIBLE", "REFUSAL"] },
                "profile_id": { "type": ["string", "null"] },
                "profile_sha256": { "type": ["string", "null"] },
                "input_verification": {},
                "files": {
                    "type": "object",
                    "required": ["old", "new"],
                    "properties": {
                        "old": { "type": "string" },
                        "new": { "type": "string" }
                    }
                },
                "dialect": {
                    "type": "object",
                    "properties": {
                        "old": {
                            "type": ["object", "null"],
                            "properties": {
                                "delimiter": { "type": "string" },
                                "quote": { "type": "string" },
                                "escape": { "type": "string" }
                            }
                        },
                        "new": {
                            "type": ["object", "null"],
                            "properties": {
                                "delimiter": { "type": "string" },
                                "quote": { "type": "string" },
                                "escape": { "type": "string" }
                            }
                        }
                    }
                },
                "checks": {
                    "type": ["object", "null"],
                    "properties": {
                        "schema_overlap": {
                            "type": "object",
                            "properties": {
                                "status": { "type": "string", "enum": ["pass", "fail"] },
                                "columns_common": { "type": "integer" },
                                "columns_old_only": { "type": "array", "items": { "type": "string" } },
                                "columns_new_only": { "type": "array", "items": { "type": "string" } },
                                "overlap_ratio": { "type": "number" }
                            }
                        },
                        "key_viability": {
                            "type": ["object", "null"],
                            "properties": {
                                "status": { "type": "string", "enum": ["pass", "fail"] },
                                "key_column": { "type": "string" },
                                "found_old": { "type": "boolean" },
                                "found_new": { "type": "boolean" },
                                "unique_old": { "type": ["boolean", "null"] },
                                "unique_new": { "type": ["boolean", "null"] },
                                "coverage": { "type": ["number", "null"] }
                            }
                        },
                        "row_granularity": {
                            "type": "object",
                            "properties": {
                                "status": { "type": "string", "enum": ["pass", "fail"] },
                                "rows_old": { "type": "integer" },
                                "rows_new": { "type": "integer" },
                                "key_overlap": { "type": ["integer", "null"] },
                                "keys_old_only": { "type": ["integer", "null"] },
                                "keys_new_only": { "type": ["integer", "null"] }
                            }
                        },
                        "type_consistency": {
                            "type": "object",
                            "properties": {
                                "status": { "type": "string", "enum": ["pass", "fail"] },
                                "numeric_columns": { "type": "integer" },
                                "type_shifts": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "column": { "type": "string" },
                                            "old_type": { "type": "string" },
                                            "new_type": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "reasons": { "type": ["array", "null"], "items": { "type": "string" } },
                "refusal": {
                    "type": ["object", "null"],
                    "properties": {
                        "code": { "type": "string" },
                        "message": { "type": "string" },
                        "detail": { "type": "object" },
                        "next_command": { "type": ["string", "null"] }
                    }
                }
            }
        });
        let mut stdout = io::stdout();
        serde_json::to_writer_pretty(&mut stdout, &schema)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        return Ok(0);
    }

    if args.describe {
        let mut stdout = io::stdout();
        stdout.write_all(include_bytes!("../operator.json"))?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        return Ok(0);
    }

    if let Some(ref command) = args.command {
        return run_witness(command);
    }

    // Emit stderr warnings for reserved v0 flags (bd-3pgf)
    emit_reserved_flag_warnings(&args);

    let result = orchestrator::run(&args)?;
    let mode = if args.json {
        cli::exit::OutputMode::Json
    } else {
        cli::exit::OutputMode::Human
    };
    let stream = cli::exit::output_stream(result.outcome, mode);

    match stream {
        cli::exit::OutputStream::Stdout => {
            let mut stdout = io::stdout();
            stdout.write_all(result.output.as_bytes())?;
            stdout.flush()?;
        }
        cli::exit::OutputStream::Stderr => {
            let mut stderr = io::stderr();
            stderr.write_all(result.output.as_bytes())?;
            stderr.flush()?;
        }
    }

    if !args.no_witness {
        witness::record_run(&args, &result);
    }

    Ok(cli::exit::exit_code(result.outcome))
}

fn emit_reserved_flag_warnings(args: &cli::args::Args) {
    use std::io::Write;
    let mut stderr = std::io::stderr();
    if !args.lock.is_empty() {
        let _ = writeln!(
            stderr,
            "shape: note: --lock accepted but lock verification is deferred in v0"
        );
    }
    if args.max_rows.is_some() {
        let _ = writeln!(
            stderr,
            "shape: note: --max-rows accepted but row-limit refusal is deferred in v0"
        );
    }
    if args.max_bytes.is_some() {
        let _ = writeln!(
            stderr,
            "shape: note: --max-bytes accepted but byte-limit refusal is deferred in v0"
        );
    }
}

fn run_witness(command: &cli::args::ShapeCommand) -> Result<u8, Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    let action = match command {
        cli::args::ShapeCommand::Witness { action } => action,
    };
    let response = cli::witness::execute(action);

    if let Some(stdout_payload) = response.stdout.as_deref() {
        let mut stdout = io::stdout();
        stdout.write_all(stdout_payload.as_bytes())?;
        stdout.flush()?;
    }

    if let Some(stderr_payload) = response.stderr.as_deref() {
        let mut stderr = io::stderr();
        stderr.write_all(stderr_payload.as_bytes())?;
        stderr.flush()?;
    }

    Ok(response.exit_code)
}
