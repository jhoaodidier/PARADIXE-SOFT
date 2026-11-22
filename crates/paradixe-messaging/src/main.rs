#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use paradixe_domain::ProposedPassability;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use paradixe_messaging::{
    bridge::CoreApiBridge,
    parser::parse_citizen_message,
    providers::MessagingConfig,
    server::{build_router, MessagingState},
};

#[derive(Parser)]
#[command(
    name = "paradixe-messaging",
    about = "WhatsApp Cloud API + SMS fallback + HMAC citizen gateway for PARADIXE SOFT"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP webhook and internal dispatch gateway
    Serve,
    /// Parse a citizen WhatsApp/SMS text command deterministically to JSON
    Parse { message: String },
    /// Forward a HMAC-signed report directly to paradixe-api /webhooks/messaging
    SimulateWebhook {
        #[arg(long, default_value = "WHATSAPP_BOT")]
        channel: String,
        #[arg(long, default_value = "+51962000100")]
        phone: String,
        #[arg(long)]
        segment_id: Uuid,
        #[arg(long, default_value = "BLOCKED")]
        passability: String,
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    match cli.command {
        Commands::Parse { message } => {
            let intent = parse_citizen_message(&message);
            println!("{}", serde_json::to_string_pretty(&intent)?);
        }
        Commands::SimulateWebhook {
            channel,
            phone,
            segment_id,
            passability,
            lat,
            lon,
        } => {
            let parsed_pass = match passability.as_str() {
                "BLOCKED" => ProposedPassability::Blocked,
                "PARTIAL_ONE_LANE" => ProposedPassability::PartialOneLane,
                "OPEN" => ProposedPassability::Open,
                other => return Err(format!("Invalid passability: {other}").into()),
            };
            let config = MessagingConfig::from_env()?;
            let bridge = CoreApiBridge::new(config);
            let res = bridge
                .forward_signed_report(&channel, &phone, segment_id, parsed_pass, lat, lon)
                .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "accepted": res.accepted,
                    "reply_preview": res.reply_preview
                }))?
            );
        }
        Commands::Serve => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info,paradixe_messaging=debug")),
                )
                .init();

            let config = MessagingConfig::from_env()?;
            let addr = config.bind_addr.clone();
            let state = MessagingState::from_config(config);
            let listener = TcpListener::bind(&addr).await?;
            tracing::info!(bind_addr = %addr, "paradixe-messaging listening");
            axum::serve(listener, build_router(state)).await?;
        }
    }

    Ok(())
}
