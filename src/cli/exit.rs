use crate::checks::suite::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

pub fn exit_code(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Compatible => 0,
        Outcome::Incompatible => 1,
        Outcome::Refusal => 2,
    }
}

pub fn output_stream(outcome: Outcome, mode: OutputMode) -> OutputStream {
    match mode {
        OutputMode::Json => OutputStream::Stdout,
        OutputMode::Human => {
            if outcome == Outcome::Refusal {
                OutputStream::Stderr
            } else {
                OutputStream::Stdout
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OutputMode, OutputStream, exit_code, output_stream};
    use crate::checks::suite::Outcome;

    #[test]
    fn maps_exit_codes_for_all_outcomes() {
        assert_eq!(exit_code(Outcome::Compatible), 0);
        assert_eq!(exit_code(Outcome::Incompatible), 1);
        assert_eq!(exit_code(Outcome::Refusal), 2);
    }

    #[test]
    fn routes_human_mode_refusal_to_stderr() {
        assert_eq!(
            output_stream(Outcome::Refusal, OutputMode::Human),
            OutputStream::Stderr
        );
    }

    #[test]
    fn routes_human_mode_non_refusals_to_stdout() {
        assert_eq!(
            output_stream(Outcome::Compatible, OutputMode::Human),
            OutputStream::Stdout
        );
        assert_eq!(
            output_stream(Outcome::Incompatible, OutputMode::Human),
            OutputStream::Stdout
        );
    }

    #[test]
    fn routes_json_mode_all_outcomes_to_stdout() {
        for outcome in [Outcome::Compatible, Outcome::Incompatible, Outcome::Refusal] {
            assert_eq!(
                output_stream(outcome, OutputMode::Json),
                OutputStream::Stdout
            );
        }
    }
}
