#![forbid(unsafe_code)]

use std::io::{self, Read};
use std::process::ExitCode;

use paradixe_consensus::{
    evaluate_field_consensus, evaluate_telemetry_cluster, ConsensusCliInput, ConsensusCliOutput,
};

fn main() -> ExitCode {
    let mut raw = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut raw) {
        eprintln!("failed to read stdin: {err}");
        return ExitCode::from(1);
    }

    let input: ConsensusCliInput = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("invalid JSON input contract: {err}");
            return ExitCode::from(2);
        }
    };

    let output = ConsensusCliOutput {
        telemetry: evaluate_telemetry_cluster(
            input.segment_id,
            &input.pings,
            input.evaluation_time,
        ),
        consensus: evaluate_field_consensus(&input.reports, input.evaluation_time),
    };

    match serde_json::to_string_pretty(&output) {
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
