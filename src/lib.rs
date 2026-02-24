#![forbid(unsafe_code)]

pub mod checks;
pub mod cli;
pub mod csv;
pub mod format;
pub mod normalize;
pub mod orchestrator;
pub mod output;
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
