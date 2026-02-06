#![forbid(unsafe_code)]

pub mod cli;
pub mod csv;
pub mod checks;
pub mod format;
pub mod normalize;
pub mod orchestrator;
pub mod output;
pub mod refusal;

/// Run the shape pipeline. Returns exit code (0, 1, or 2).
pub fn run() -> Result<u8, Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    let args = match cli::args::Args::parse() {
        Ok(args) => args,
        Err(err) => {
            err.print()?;
            return Ok(2);
        }
    };

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

    Ok(cli::exit::exit_code(result.outcome))
}
