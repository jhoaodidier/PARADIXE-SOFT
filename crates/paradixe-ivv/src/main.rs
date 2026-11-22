#![forbid(unsafe_code)]

use std::io::{self, Read};
use std::process::ExitCode;

use paradixe_ivv::{calculate_ivv, IvvCliInput};

fn main() -> ExitCode {
    let mut raw = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut raw) {
        eprintln!("failed to read stdin: {err}");
        return ExitCode::from(1);
    }

    let input: IvvCliInput = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("invalid JSON input contract: {err}");
            return ExitCode::from(2);
        }
    };

    match calculate_ivv(
        &input.model,
        &input.segment,
        &input.observation,
        input.calculated_at,
        input.evaluation_time,
    ) {
        Ok(output) => match serde_json::to_string_pretty(&output) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("failed to serialize output: {err}");
                ExitCode::from(1)
            }
        },
        Err(err) => {
            eprintln!("validation error: {err}");
            ExitCode::from(2)
        }
    }
}
