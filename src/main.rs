#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match shape::run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("shape: {e}");
            ExitCode::from(2)
        }
    }
}
