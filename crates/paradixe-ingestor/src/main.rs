#![forbid(unsafe_code)]

pub mod dem;
pub mod hotspots;
pub mod osm;
pub mod pbf_download;
pub mod pmtiles_writer;
pub mod regions;
pub mod s3;
pub mod weather;

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "paradixe-ingestor",
    about = "Scalable multi-region OSM, DEM, Open-Meteo IVV and PMTiles v3 pipeline for PARADIXE SOFT"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest OSM road network, towns, hospitals, districts and waterways into PostGIS
    OsmIngest {
        #[arg(long, default_value = "PE-HUC")]
        region: String,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
        #[arg(long, default_value = "data/raw/peru-latest.osm.pbf")]
        pbf: PathBuf,
        #[arg(long)]
        download: bool,
        #[arg(long)]
        force_download: bool,
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "motorway,trunk,primary,secondary,tertiary,unclassified,motorway_link,trunk_link,primary_link,secondary_link"
        )]
        highway_classes: Vec<String>,
    },
    /// Enrich road_nodes elevation_m and road_segments directional slopes from Copernicus GLO-90 DEM
    DemEnrich {
        #[arg(long, default_value = "PE-HUC")]
        region: String,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
        #[arg(long)]
        refresh_all: bool,
        #[arg(long)]
        max_nodes: Option<usize>,
    },
    /// Sync 24h/72h Open-Meteo rainfall observations and recalculate deterministic IVV scores
    WeatherSync {
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
        #[arg(long)]
        once: bool,
        #[arg(long, default_value_t = 2)]
        interval_hours: u64,
    },
    /// Generate PMTiles v3 vector archive from PostGIS ST_AsMVT and upload to S3 / RustFS
    TilesPublish {
        #[arg(long, default_value = "PE-HUC")]
        region: String,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
        #[arg(long, default_value_t = 6)]
        min_zoom: u8,
        #[arg(long, default_value_t = 10)]
        max_zoom: u8,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        skip_upload: bool,
    },
    /// Mark INGEMMET/Provias documented historical landslide hotspots and soil susceptibility
    HotspotsSync {
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
    },
    /// Generate +24h/+48h/+72h predictive IVV risk forecasts from Open-Meteo rainfall forecasts
    ForecastSync {
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
    },
    /// Execute full regional onboarding pipeline: OSM -> DEM -> Hotspots -> Weather/IVV/Forecast -> PMTiles v3 on RustFS/S3
    SyncAll {
        #[arg(long, default_value = "PE-HUC")]
        region: String,
        #[arg(long)]
        bbox: Option<String>,
        #[arg(long)]
        region_name: Option<String>,
        #[arg(long, default_value = "data/raw/peru-latest.osm.pbf")]
        pbf: PathBuf,
        #[arg(long)]
        download: bool,
        #[arg(long)]
        max_nodes: Option<usize>,
        #[arg(long, default_value_t = 6)]
        min_zoom: u8,
        #[arg(long, default_value_t = 10)]
        max_zoom: u8,
    },
}

async fn connect_pool() -> Result<sqlx::PgPool, String> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set".to_string())?;
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await
        .map_err(|e| format!("Failed connecting to PostgreSQL/PostGIS: {e}"))
}

async fn publish_tiles_step(
    pool: &sqlx::PgPool,
    region: &regions::RegionSpec,
    min_zoom: u8,
    max_zoom: u8,
    out_opt: Option<PathBuf>,
    skip_upload: bool,
) -> Result<(PathBuf, usize, usize, Option<String>), String> {
    let out_path =
        out_opt.unwrap_or_else(|| PathBuf::from(format!("data/processed/{}.pmtiles", region.slug)));
    let (bytes, tile_count) = pmtiles_writer::build_region_pmtiles_from_postgis(
        pool, region, min_zoom, max_zoom, &out_path,
    )
    .await?;
    let byte_len = bytes.len();

    let uri_opt = if skip_upload {
        None
    } else {
        let s3_cfg = s3::S3Config::from_env()?;
        let s3 = s3::S3Client::new(s3_cfg.clone())?;
        s3.ensure_bucket(&s3_cfg.bucket_tiles, true).await?;
        let key = format!("{}.pmtiles", region.slug);
        let public_url = s3
            .put_object(&s3_cfg.bucket_tiles, &key, "application/vnd.pmtiles", bytes)
            .await?;

        sqlx::query(
            "UPDATE territorial_units SET pmtiles_uri = $2, updated_at = NOW() WHERE iso_code = $1",
        )
        .bind(&region.iso_code)
        .bind(&public_url)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        Some(public_url)
    };

    Ok((out_path, tile_count, byte_len, uri_opt))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,paradixe_ingestor=info,sqlx=warn")),
        )
        .init();

    let cli = Cli::parse();
    let pool = connect_pool().await?;

    match cli.command {
        Commands::OsmIngest {
            region,
            bbox,
            region_name,
            pbf,
            download,
            force_download,
            highway_classes,
        } => {
            let reg = regions::resolve_region(&region, bbox.as_deref(), region_name.as_deref())?;
            if download || force_download || !pbf.exists() {
                pbf_download::ensure_peru_pbf(&pbf, force_download).await?;
            }
            let stats = osm::ingest_osm_region(&pool, &pbf, &reg, &highway_classes).await?;
            tracing::info!(?stats, region = %reg.iso_code, "OSM ingest completed");
        }
        Commands::DemEnrich {
            region,
            bbox,
            region_name,
            refresh_all,
            max_nodes,
        } => {
            let reg = regions::resolve_region(&region, bbox.as_deref(), region_name.as_deref())?;
            let (nodes_up, segs_up) =
                dem::enrich_dem_elevations_and_slopes(&pool, &reg, refresh_all, max_nodes).await?;
            tracing::info!(region = %reg.iso_code, nodes_up, segs_up, "DEM elevation & slope enrichment completed");
        }
        Commands::WeatherSync {
            region,
            bbox,
            region_name,
            once,
            interval_hours,
        } => {
            let reg = match region {
                Some(r) => Some(regions::resolve_region(
                    &r,
                    bbox.as_deref(),
                    region_name.as_deref(),
                )?),
                None => None,
            };
            loop {
                let stats = weather::sync_weather_and_ivv(&pool, reg.as_ref()).await?;
                tracing::info!(?stats, "Weather IVV sync completed");
                let forecast_stats = weather::sync_weather_forecasts(&pool, reg.as_ref()).await?;
                tracing::info!(
                    ?forecast_stats,
                    "Predictive 72h IVV forecast sync completed"
                );
                if once {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(interval_hours.max(1) * 3600)).await;
            }
        }
        Commands::HotspotsSync {
            region,
            bbox,
            region_name,
        } => {
            let reg = match region {
                Some(r) => Some(regions::resolve_region(
                    &r,
                    bbox.as_deref(),
                    region_name.as_deref(),
                )?),
                None => None,
            };
            let stats = hotspots::sync_historical_hotspots(&pool, reg.as_ref()).await?;
            tracing::info!(?stats, "Andean geotechnical hotspots sync completed");
        }
        Commands::ForecastSync {
            region,
            bbox,
            region_name,
        } => {
            let reg = match region {
                Some(r) => Some(regions::resolve_region(
                    &r,
                    bbox.as_deref(),
                    region_name.as_deref(),
                )?),
                None => None,
            };
            let stats = weather::sync_weather_forecasts(&pool, reg.as_ref()).await?;
            tracing::info!(?stats, "Predictive 72h IVV forecast sync completed");
        }
        Commands::TilesPublish {
            region,
            bbox,
            region_name,
            min_zoom,
            max_zoom,
            out,
            skip_upload,
        } => {
            let reg = regions::resolve_region(&region, bbox.as_deref(), region_name.as_deref())?;
            let (out_path, tile_count, byte_len, uri) =
                publish_tiles_step(&pool, &reg, min_zoom, max_zoom, out, skip_upload).await?;
            tracing::info!(
                region = %reg.iso_code,
                path = %out_path.display(),
                tile_count,
                byte_len,
                s3_uri = ?uri,
                "PMTiles publish completed"
            );
        }
        Commands::SyncAll {
            region,
            bbox,
            region_name,
            pbf,
            download,
            max_nodes,
            min_zoom,
            max_zoom,
        } => {
            let reg = regions::resolve_region(&region, bbox.as_deref(), region_name.as_deref())?;
            if download || !pbf.exists() {
                pbf_download::ensure_peru_pbf(&pbf, false).await?;
            }
            let default_hw: Vec<String> = osm::DEFAULT_HIGHWAY_CLASSES
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            let osm_stats = osm::ingest_osm_region(&pool, &pbf, &reg, &default_hw).await?;
            tracing::info!(?osm_stats, region = %reg.iso_code, "Step 1/4 OSM ingest completed");

            let (nodes_up, segs_up) =
                dem::enrich_dem_elevations_and_slopes(&pool, &reg, false, max_nodes).await?;
            tracing::info!(nodes_up, segs_up, "Step 2/4 DEM enrichment completed");

            let hotspot_stats = hotspots::sync_historical_hotspots(&pool, Some(&reg)).await?;
            tracing::info!(?hotspot_stats, "Andean geotechnical hotspots marked");

            let weather_stats = weather::sync_weather_and_ivv(&pool, Some(&reg)).await?;
            tracing::info!(?weather_stats, "Step 3/4 Weather IVV sync completed");

            let forecast_stats = weather::sync_weather_forecasts(&pool, Some(&reg)).await?;
            tracing::info!(
                ?forecast_stats,
                "Predictive 72h IVV forecast sync completed"
            );

            let (out_path, tile_count, byte_len, uri) =
                publish_tiles_step(&pool, &reg, min_zoom, max_zoom, None, false).await?;
            tracing::info!(
                path = %out_path.display(),
                tile_count,
                byte_len,
                s3_uri = ?uri,
                "Step 4/4 PMTiles publish completed"
            );
        }
    }

    Ok(())
}
