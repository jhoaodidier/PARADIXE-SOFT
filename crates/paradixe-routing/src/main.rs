#![forbid(unsafe_code)]

use std::io::{self, Read};
use std::process::ExitCode;

use paradixe_routing::{calculate_route, RoutingInput};

fn main() -> ExitCode {
    let mut raw = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut raw) {
        eprintln!("failed to read stdin: {err}");
        return ExitCode::from(1);
    }

    let input: RoutingInput = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("invalid JSON input contract: {err}");
            return ExitCode::from(2);
        }
    };

    let resolution = calculate_route(&input);
    match serde_json::to_string_pretty(&resolution) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to serialize output: {err}");
            ExitCode::from(1)
        }
    }
}
