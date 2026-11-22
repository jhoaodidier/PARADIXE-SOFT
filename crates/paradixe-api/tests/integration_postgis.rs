#![forbid(unsafe_code)]

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use paradixe_api::{
    auth::{compute_otp_hmac_hex, encode_access_token, hash_password, RoleCode},
    build_router,
    config::AppConfig,
    fixtures::{
        DRILL_FIXTURE_TAG, NODE_AMBO_ID, NODE_CARPISH_NORTE_ID, NODE_CARPISH_SUR_ID,
        NODE_HUANUCO_ID, NODE_TINGO_MARIA_ID, SEG_BYPASS_AMBO_ID, SEG_BYPASS_TINGO_ID,
        SEG_CARPISH_NORTE_TINGO_ID, SEG_CARPISH_TUNNEL_ID, SEG_HUANUCO_CARPISH_SUR_ID,
        SIM_TRANSFER_CORRIDOR_ID,
    },
    incidents::DailyTelemetryKeyStore,
    sync::append_sync_change,
    AppState, OperationalPreviewGateway, SimulationNotificationGateway,
};
use rust_decimal::Decimal;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: Router,
    pool: PgPool,
    sim_pool: PgPool,
    config: Arc<AppConfig>,
    key_dir: std::path::PathBuf,
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.key_dir);
    }
}

async fn setup_harness() -> TestHarness {
    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for ignored PostGIS tests");
    let simulator_database_url = std::env::var("SIMULATOR_DATABASE_URL")
        .expect("SIMULATOR_DATABASE_URL must be set for ignored PostGIS tests");

    let key_dir = std::env::temp_dir().join(format!("paradixe_keys_{}", Uuid::new_v4()));
    let config = Arc::new(AppConfig {
        api_bind_addr: "127.0.0.1:0".to_string(),
        database_url: SecretString::from(database_url.clone()),
        simulator_database_url: SecretString::from(simulator_database_url.clone()),
        jwt_secret: SecretString::from("test_jwt_secret_at_least_32_bytes_for_postgis_suite!!"),
        jwt_access_ttl_min: 15,
        jwt_refresh_ttl_days: 30,
        otp_secret: SecretString::from("test_otp_secret_at_least_32_bytes_for_postgis_suite!!"),
        webhook_secret: SecretString::from("test_webhook_secret_at_least_32_bytes!!"),
        telemetry_key_store_dir: key_dir.clone(),
    });

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .expect("connect DATABASE_URL");

    let sim_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&simulator_database_url)
        .await
        .expect("connect SIMULATOR_DATABASE_URL");

    sqlx::query(
        "TRUNCATE TABLE
            simulation_impact_logs, sim_events, simulation_scenarios,
            humanitarian_windows, logistic_bridges, incident_report_links,
            incidents, field_reports, telemetry_pings, telemetry_daily_salts,
            ivv_assessments, weather_observations, sync_change_log,
            transfer_corridor_capabilities, transfer_corridors,
            road_segments, road_nodes, audit_logs, otp_challenges,
            auth_refresh_tokens, users
         RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .expect("clean operational tables");

    sqlx::query("UPDATE sync_control SET last_seq = 0 WHERE singleton_id = 1")
        .execute(&pool)
        .await
        .expect("reset sync_control");

    let telemetry_keys = DailyTelemetryKeyStore::new(key_dir.clone()).expect("key store");
    let state = AppState {
        pool: pool.clone(),
        sim_pool: sim_pool.clone(),
        config: config.clone(),
        rate_limiter: paradixe_api::auth::RateLimiter::default(),
        telemetry_keys,
        gateway: Arc::new(OperationalPreviewGateway),
        sim_gateway: Arc::new(SimulationNotificationGateway::default()),
    };

    TestHarness {
        app: build_router(state),
        pool,
        sim_pool,
        config,
        key_dir,
    }
}

async fn seed_user(pool: &PgPool, role_code: &str, phone: &str, password: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    let pw_hash = hash_password(password).expect("hash password");
    sqlx::query(
        "INSERT INTO users (user_id, role_id, phone_e164, full_name, password_hash, reputation_score, is_active)
         SELECT $1, role_id, $2, $3, $4, 0.80, TRUE
         FROM roles
         WHERE code = $5",
    )
    .bind(user_id)
    .bind(phone)
    .bind(format!("Test {role_code}"))
    .bind(pw_hash)
    .bind(role_code)
    .execute(pool)
    .await
    .expect("insert user");
    user_id
}

async fn send_json(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {tok}"));
    }
    let req = if let Some(json_body) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.expect("oneshot request");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, parsed)
}

async fn login_password(
    app: &Router,
    phone: &str,
    password: &str,
    device_id: &str,
) -> (String, String) {
    let (status, val) = send_json(
        app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({
            "phone_e164": phone,
            "device_id": device_id,
            "password": password
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {val}");
    (
        val["access_token"].as_str().unwrap().to_string(),
        val["refresh_token"].as_str().unwrap().to_string(),
    )
}

async fn insert_node(
    pool: &PgPool,
    node_id: Uuid,
    name: &str,
    lon: f64,
    lat: f64,
    urban_radius_m: i32,
) {
    sqlx::query(
        "INSERT INTO road_nodes (node_id, name, node_type, location_geom, elevation_m, is_geotechnically_stable, urban_radius_m)
         VALUES ($1, $2, 'INTERSECTION', ST_SetSRID(ST_MakePoint($3, $4), 4326), 1900, TRUE, $5)",
    )
    .bind(node_id)
    .bind(name)
    .bind(lon)
    .bind(lat)
    .bind(urban_radius_m)
    .execute(pool)
    .await
    .expect("insert node");
}

#[allow(clippy::too_many_arguments)]
async fn insert_segment(
    pool: &PgPool,
    segment_id: Uuid,
    origin_node_id: Uuid,
    destination_node_id: Uuid,
    wkt_linestring: &str,
    slope_fwd: &str,
    slope_rev: Option<&str>,
    verified: bool,
) {
    sqlx::query(
        "INSERT INTO road_segments (
            segment_id, origin_node_id, destination_node_id, corridor_name, geom,
            direction_type, length_km, avg_slope_pct, max_slope_forward_pct, max_slope_reverse_pct,
            road_width_m, max_supported_tons, surface_type, soil_susceptibility_idx,
            distance_to_ravine_m, is_historical_hotspot, base_travel_time_min,
            is_topology_verified, geometry_source, attributes_verified_at
         ) VALUES (
            $1, $2, $3, 'Huanuco-Carpish', ST_GeomFromText($4, 4326),
            $5, 10.00, 5.0, $6, $7,
            6.50, 40.00, 'PAVED', 0.80,
            200, TRUE, 20.00,
            TRUE, 'FIELD_SURVEY', $8
         )",
    )
    .bind(segment_id)
    .bind(origin_node_id)
    .bind(destination_node_id)
    .bind(wkt_linestring)
    .bind(if slope_rev.is_some() {
        "BIDIRECTIONAL"
    } else {
        "ONE_WAY"
    })
    .bind(Decimal::from_str(slope_fwd).unwrap())
    .bind(slope_rev.map(|s| Decimal::from_str(s).unwrap()))
    .bind(if verified { Some(Utc::now()) } else { None })
    .execute(pool)
    .await
    .expect("insert segment");
}

async fn attach_fresh_ivv(
    pool: &PgPool,
    segment_id: Uuid,
    precip_24: f64,
    precip_72: f64,
    score: &str,
) {
    let obs_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO weather_observations (observation_id, segment_id, source_name, precip_24h_mm, precip_72h_mm, observed_at)
         VALUES ($1, $2, 'SENAMHI', $3, $4, NOW())",
    )
    .bind(obs_id)
    .bind(segment_id)
    .bind(Decimal::try_from(precip_24).unwrap())
    .bind(Decimal::try_from(precip_72).unwrap())
    .execute(pool)
    .await
    .expect("insert weather obs");

    sqlx::query(
        "INSERT INTO ivv_assessments (assessment_id, observation_id, model_version_id, ivv_score, calculated_at)
         SELECT $1, $2, model_version_id, $3, NOW()
         FROM ivv_model_versions WHERE is_active = TRUE LIMIT 1",
    )
    .bind(Uuid::new_v4())
    .bind(obs_id)
    .bind(Decimal::from_str(score).unwrap())
    .execute(pool)
    .await
    .expect("insert ivv");
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_1_sec_auth_rbac_refresh_reuse_otp_webhook_telemetry_privacy() {
    let h = setup_harness().await;
    let driver_id = seed_user(&h.pool, "DRIVER", "+51900000001", "PassDriver123!").await;
    let _coer_id = seed_user(&h.pool, "COER_ADMIN", "+51900000006", "PassCoer123!").await;

    // SEC-01: Missing bearer JWT -> 401
    let (st, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/routing/calculate",
        None,
        Some(json!({
            "origin_node_id": Uuid::new_v4(),
            "destination_node_id": Uuid::new_v4(),
            "vehicle_category_code": "AMBULANCE_4X4",
            "cargo_type": "MEDICAL_PATIENT",
            "patient_count": 1
        })),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    let (driver_tok, refresh1) =
        login_password(&h.app, "+51900000001", "PassDriver123!", "android-1").await;

    // SEC-02: DRIVER forbidden on incident verification & simulator scenarios -> 403
    let (st_verif, _) = send_json(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/incidents/{}/verification", Uuid::new_v4()),
        Some(&driver_tok),
        Some(json!({
            "verification_state": "CONFIRMED",
            "impact_passability": "BLOCKED"
        })),
    )
    .await;
    assert_eq!(st_verif, StatusCode::FORBIDDEN);

    let (st_sim, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/simulator/scenarios",
        Some(&driver_tok),
        Some(json!({
            "name": "Unauthorized",
            "fixture_tag": DRILL_FIXTURE_TAG
        })),
    )
    .await;
    assert_eq!(st_sim, StatusCode::FORBIDDEN);

    // SEC-03: Refresh token rotation + reuse revokes entire family + audit log
    let (st_rot, rot_val) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        Some(json!({
            "refresh_token": refresh1,
            "device_id": "android-1"
        })),
    )
    .await;
    assert_eq!(st_rot, StatusCode::OK);
    let refresh2 = rot_val["refresh_token"].as_str().unwrap().to_string();

    let (st_reuse, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        Some(json!({
            "refresh_token": refresh1,
            "device_id": "android-1"
        })),
    )
    .await;
    assert_eq!(st_reuse, StatusCode::UNAUTHORIZED);

    let (st_after_family_revoked, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/refresh",
        None,
        Some(json!({
            "refresh_token": refresh2,
            "device_id": "android-1"
        })),
    )
    .await;
    assert_eq!(st_after_family_revoked, StatusCode::UNAUTHORIZED);

    let audit_row = sqlx::query(
        "SELECT action, after_state_json::text AS payload
         FROM audit_logs
         WHERE actor_user_id = $1 AND action = 'REFRESH_TOKEN_REUSE_REVOKED'",
    )
    .bind(driver_id)
    .fetch_one(&h.pool)
    .await
    .expect("audit log created");
    let audit_payload: String = audit_row.try_get("payload").unwrap();
    assert!(!audit_payload.contains(&refresh1));
    assert!(!audit_payload.contains(&refresh2));

    // SEC-06: Indistinguishable OTP request + constant-time verify + single-use consumption
    let (st_otp_unknown, v_unk) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/otp/request",
        None,
        Some(json!({ "phone_e164": "+51999888777" })),
    )
    .await;
    let (st_otp_known, v_known) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/otp/request",
        None,
        Some(json!({ "phone_e164": "+51900000001" })),
    )
    .await;
    assert_eq!(st_otp_unknown, StatusCode::ACCEPTED);
    assert_eq!(st_otp_known, StatusCode::ACCEPTED);
    assert_eq!(v_unk["message"], v_known["message"]);

    let ch_id = Uuid::new_v4();
    let otp_hash = compute_otp_hmac_hex(
        h.config.otp_secret.expose_secret().as_bytes(),
        ch_id,
        "+51900000001",
        "654321",
    );
    sqlx::query(
        "INSERT INTO otp_challenges (challenge_id, phone_e164, code_hash, attempts, expires_at)
         VALUES ($1, '+51900000001', $2, 0, NOW() + INTERVAL '5 minutes')",
    )
    .bind(ch_id)
    .bind(&otp_hash)
    .execute(&h.pool)
    .await
    .unwrap();

    let (st_bad_otp, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({
            "phone_e164": "+51900000001",
            "device_id": "android-2",
            "challenge_id": ch_id,
            "otp_code": "000000"
        })),
    )
    .await;
    assert_eq!(st_bad_otp, StatusCode::UNAUTHORIZED);

    let (st_ok_otp, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({
            "phone_e164": "+51900000001",
            "device_id": "android-2",
            "challenge_id": ch_id,
            "otp_code": "654321"
        })),
    )
    .await;
    assert_eq!(st_ok_otp, StatusCode::OK);

    // Reusing consumed OTP -> 401
    let (st_reuse_otp, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/auth/login",
        None,
        Some(json!({
            "phone_e164": "+51900000001",
            "device_id": "android-2",
            "challenge_id": ch_id,
            "otp_code": "654321"
        })),
    )
    .await;
    assert_eq!(st_reuse_otp, StatusCode::UNAUTHORIZED);

    // Setup segment for webhook & telemetry privacy checks
    let n1 = Uuid::new_v4();
    let n2 = Uuid::new_v4();
    let seg = Uuid::new_v4();
    insert_node(&h.pool, n1, "N1", -76.000, -9.900, 0).await;
    insert_node(&h.pool, n2, "N2", -75.991, -9.900, 0).await;
    insert_segment(
        &h.pool,
        seg,
        n1,
        n2,
        "LINESTRING(-76.000 -9.900, -75.991 -9.900)",
        "6.0",
        Some("6.0"),
        true,
    )
    .await;

    // SEC-04: Messaging webhook HMAC & 5 min anti-replay window
    let stale_payload = serde_json::to_vec(&json!({
        "channel": "WHATSAPP_BOT",
        "sender_phone_e164": "+51900000001",
        "segment_id": seg,
        "proposed_passability": "BLOCKED",
        "latitude": -9.900,
        "longitude": -75.995,
        "timestamp_utc": (Utc::now() - Duration::minutes(10)).to_rfc3339()
    }))
    .unwrap();
    let mut mac =
        Hmac::<Sha256>::new_from_slice(h.config.webhook_secret.expose_secret().as_bytes()).unwrap();
    mac.update(&stale_payload);
    let stale_sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    let resp_stale = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/webhooks/messaging")
                .header(CONTENT_TYPE, "application/json")
                .header("X-Webhook-Signature", &stale_sig)
                .body(Body::from(stale_payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_stale.status(), StatusCode::UNAUTHORIZED);

    let fresh_payload = serde_json::to_vec(&json!({
        "channel": "WHATSAPP_BOT",
        "sender_phone_e164": "+51900000001",
        "segment_id": seg,
        "proposed_passability": "BLOCKED",
        "latitude": -9.900,
        "longitude": -75.995,
        "timestamp_utc": Utc::now().to_rfc3339()
    }))
    .unwrap();
    let mut mac_ok =
        Hmac::<Sha256>::new_from_slice(h.config.webhook_secret.expose_secret().as_bytes()).unwrap();
    mac_ok.update(&fresh_payload);
    let ok_sig = format!("sha256={}", hex::encode(mac_ok.finalize().into_bytes()));

    let resp_ok = h
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/webhooks/messaging")
                .header(CONTENT_TYPE, "application/json")
                .header("X-Webhook-Signature", ok_sig)
                .body(Body::from(fresh_payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_ok.status(), StatusCode::OK);

    // SEC-05 & SEC-07: Pseudonymous telemetry storage, symbolic key_ref only, >24h purge
    let eph = Uuid::new_v4();
    let ping_id = Uuid::new_v4();
    let now = Utc::now();
    let (st_ping, v_ping) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/telemetry/pings",
        Some(&driver_tok),
        Some(json!({
            "client_ephemeral_id": eph,
            "ephemeral_id_date_utc": now.date_naive().to_string(),
            "pings": [{
                "client_ping_uuid": ping_id,
                "latitude": -9.900,
                "longitude": -75.995,
                "speed_kmh": 1.2,
                "recorded_at": now.to_rfc3339()
            }]
        })),
    )
    .await;
    assert_eq!(st_ping, StatusCode::ACCEPTED);
    assert_eq!(v_ping["accepted"], 1);

    let row = sqlx::query(
        "SELECT anonymous_device_hash FROM telemetry_pings WHERE client_ping_uuid = $1",
    )
    .bind(ping_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let stored_hash: String = row.try_get("anonymous_device_hash").unwrap();
    assert_ne!(stored_hash, eph.to_string());
    assert_eq!(stored_hash.len(), 64);

    let salt_row =
        sqlx::query("SELECT key_ref FROM telemetry_daily_salts WHERE salt_date_utc = $1")
            .bind(now.date_naive())
            .fetch_one(&h.pool)
            .await
            .unwrap();
    let key_ref: String = salt_row.try_get("key_ref").unwrap();
    assert!(key_ref.starts_with("file:key_"));

    // Simulate >24h old synced row and verify next ingest purges it
    sqlx::query("UPDATE telemetry_pings SET synced_at = NOW() - INTERVAL '25 hours'")
        .execute(&h.pool)
        .await
        .unwrap();
    let old_time = now - Duration::hours(25);
    let (st_exp, v_exp) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/telemetry/pings",
        Some(&driver_tok),
        Some(json!({
            "client_ephemeral_id": eph,
            "ephemeral_id_date_utc": old_time.date_naive().to_string(),
            "pings": [{
                "client_ping_uuid": Uuid::new_v4(),
                "latitude": -9.900,
                "longitude": -75.995,
                "speed_kmh": 1.0,
                "recorded_at": old_time.to_rfc3339()
            }]
        })),
    )
    .await;
    assert_eq!(st_exp, StatusCode::ACCEPTED);
    assert_eq!(v_exp["expired"], 1);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM telemetry_pings")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_2_geo_postgis_geography_topology_reports_and_overpass() {
    let h = setup_harness().await;
    seed_user(&h.pool, "DRIVER", "+51900000001", "PassDriver123!").await;
    let (driver_tok, _) = login_password(&h.app, "+51900000001", "PassDriver123!", "dev-geo").await;

    // Nodes 1000m apart along lat = -9.900
    let n1 = Uuid::new_v4();
    let n2 = Uuid::new_v4();
    insert_node(&h.pool, n1, "Origin", -76.000, -9.900, 0).await;
    insert_node(&h.pool, n2, "Dest", -75.991, -9.900, 0).await;

    // 14m offset east (~0.0001275 deg lon at -9.9 lat) vs 16m (~0.0001458 deg lon)
    let seg_14m = Uuid::new_v4();
    let seg_16m = Uuid::new_v4();
    insert_segment(
        &h.pool,
        seg_14m,
        n1,
        n2,
        "LINESTRING(-75.999872 -9.900, -75.991 -9.900)",
        "5.0",
        Some("5.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        seg_16m,
        n1,
        n2,
        "LINESTRING(-75.999854 -9.900, -75.991 -9.900)",
        "5.0",
        Some("5.0"),
        true,
    )
    .await;

    // GEO-01: Evaluate canonical PostGIS metric topology verification
    sqlx::query(
        "UPDATE road_segments rs
         SET is_topology_verified = (
             ST_IsValid(rs.geom)
             AND ST_IsSimple(rs.geom)
             AND ST_Length(rs.geom::geography) > 0
             AND ST_DWithin(ST_StartPoint(rs.geom)::geography, o.location_geom::geography, 15)
             AND ST_DWithin(ST_EndPoint(rs.geom)::geography, d.location_geom::geography, 15)
         )
         FROM road_nodes o, road_nodes d
         WHERE o.node_id = rs.origin_node_id AND d.node_id = rs.destination_node_id",
    )
    .execute(&h.pool)
    .await
    .unwrap();

    let (st_map, map_val) =
        send_json(&h.app, Method::GET, "/api/v1/corridors/map", None, None).await;
    assert_eq!(st_map, StatusCode::OK);
    let features = map_val["features"].as_array().unwrap();
    assert_eq!(features.len(), 1);
    assert_eq!(
        features[0]["properties"]["segment_id"].as_str().unwrap(),
        seg_14m.to_string()
    );

    // GEO-02: Compute 490m vs 510m northward offsets via PostGIS ST_Project(geography)
    let pts: (f64, f64, f64, f64) = sqlx::query_as(
        "SELECT
            ST_X(ST_Project(ST_SetSRID(ST_MakePoint(-75.995, -9.900), 4326)::geography, 490, 0)::geometry),
            ST_Y(ST_Project(ST_SetSRID(ST_MakePoint(-75.995, -9.900), 4326)::geography, 490, 0)::geometry),
            ST_X(ST_Project(ST_SetSRID(ST_MakePoint(-75.995, -9.900), 4326)::geography, 510, 0)::geometry),
            ST_Y(ST_Project(ST_SetSRID(ST_MakePoint(-75.995, -9.900), 4326)::geography, 510, 0)::geometry)",
    )
    .fetch_one(&h.pool)
    .await
    .unwrap();

    let (st_490, v_490) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/reports",
        Some(&driver_tok),
        Some(json!({
            "client_uuid": Uuid::new_v4(),
            "segment_id": seg_14m,
            "proposed_passability": "BLOCKED",
            "latitude": pts.1,
            "longitude": pts.0,
            "captured_at_client": Utc::now().to_rfc3339()
        })),
    )
    .await;
    assert_eq!(st_490, StatusCode::CREATED);
    assert_eq!(v_490["validation_status"], "PENDING");

    let (st_510, v_510) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/reports",
        Some(&driver_tok),
        Some(json!({
            "client_uuid": Uuid::new_v4(),
            "segment_id": seg_14m,
            "proposed_passability": "BLOCKED",
            "latitude": pts.3,
            "longitude": pts.2,
            "captured_at_client": Utc::now().to_rfc3339()
        })),
    )
    .await;
    assert_eq!(st_510, StatusCode::CREATED);
    assert_eq!(v_510["validation_status"], "REJECTED_DISTANCE");

    // GEO-03: Overpass crossing in 2D without shared node does not connect graph
    let n3 = Uuid::new_v4();
    let n4 = Uuid::new_v4();
    insert_node(&h.pool, n3, "OverpassS", -75.995, -9.905, 0).await;
    insert_node(&h.pool, n4, "OverpassN", -75.995, -9.895, 0).await;
    let seg_cross = Uuid::new_v4();
    insert_segment(
        &h.pool,
        seg_cross,
        n3,
        n4,
        "LINESTRING(-75.995 -9.905, -75.995 -9.895)",
        "4.0",
        Some("4.0"),
        true,
    )
    .await;
    attach_fresh_ivv(&h.pool, seg_14m, 5.0, 10.0, "15.00").await;
    attach_fresh_ivv(&h.pool, seg_cross, 5.0, 10.0, "15.00").await;

    let (st_cross, v_cross) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/routing/calculate",
        Some(&driver_tok),
        Some(json!({
            "origin_node_id": n1,
            "destination_node_id": n4,
            "vehicle_category_code": "PICKUP_4X4",
            "cargo_type": "MEDICAL_SUPPLIES",
            "cargo_payload_kg": 100
        })),
    )
    .await;
    assert_eq!(st_cross, StatusCode::OK);
    assert_eq!(v_cross["resolution_type"], "NO_SAFE_ROUTE");
    assert_eq!(v_cross["reason_code"], "NETWORK_DISCONNECTED");
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_3_ivv_and_corridors_map_stale_and_revalidation() {
    let h = setup_harness().await;
    let coer_id = seed_user(&h.pool, "COER_ADMIN", "+51900000006", "PassCoer123!").await;
    let (coer_tok, _) = login_password(&h.app, "+51900000006", "PassCoer123!", "coer-1").await;

    let n1 = Uuid::new_v4();
    let n2 = Uuid::new_v4();
    let seg = Uuid::new_v4();
    let corr = Uuid::new_v4();
    insert_node(&h.pool, n1, "CarpishSur", -76.091, -9.715, 0).await;
    insert_node(&h.pool, n2, "CarpishNorte", -76.091, -9.705, 0).await;
    insert_segment(
        &h.pool,
        seg,
        n1,
        n2,
        "LINESTRING(-76.091 -9.715, -76.091 -9.705)",
        "27.0",
        Some("27.0"),
        true,
    )
    .await;

    sqlx::query(
        "INSERT INTO transfer_corridors (
            corridor_id, side_a_node_id, side_b_node_id, path_geom, length_m, max_slope_pct,
            transfer_mode, max_safe_ivv, is_verified_safe, last_inspected_at,
            inspection_valid_days, requires_revalidation, verified_by_user_id
         ) VALUES (
            $1, $2, $3, ST_GeomFromText('LINESTRING(-76.091 -9.715, -76.091 -9.705)', 4326),
            450, 12.0, 'PEDESTRIAN_SAFE_PATH', 70.00, TRUE, NOW(), 30, FALSE, $4
         )",
    )
    .bind(corr)
    .bind(n1)
    .bind(n2)
    .bind(coer_id)
    .execute(&h.pool)
    .await
    .unwrap();

    // IVV-01: Extreme rain 85/190 mm triggers IVV 84.00 RED and requires_revalidation = TRUE
    let (st_ivv, v_ivv) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/ivv/recalculate",
        Some(&coer_tok),
        Some(json!({
            "observations": [{
                "segment_id": seg,
                "source_name": "SENAMHI",
                "precip_24h_mm": 85.0,
                "precip_72h_mm": 190.0,
                "observed_at": Utc::now().to_rfc3339()
            }]
        })),
    )
    .await;
    assert_eq!(st_ivv, StatusCode::CREATED);
    assert_eq!(v_ivv["assessments_created"], 1);
    assert_eq!(v_ivv["revalidations_triggered"], 1);

    let reval: bool = sqlx::query_scalar(
        "SELECT requires_revalidation FROM transfer_corridors WHERE corridor_id = $1",
    )
    .bind(corr)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert!(reval);

    let (st_map, v_map) = send_json(&h.app, Method::GET, "/api/v1/corridors/map", None, None).await;
    assert_eq!(st_map, StatusCode::OK);
    assert_eq!(v_map["features"][0]["properties"]["ivv_score"], 84.0);
    assert_eq!(v_map["features"][0]["properties"]["risk_level"], "RED");
    assert_eq!(v_map["features"][0]["properties"]["is_stale"], false);

    // IVV-02: Expire assessment by 3h 1s -> UNKNOWN/is_stale in map + STALE_IVV in routing
    sqlx::query("UPDATE ivv_assessments SET calculated_at = NOW() - INTERVAL '10801 seconds'")
        .execute(&h.pool)
        .await
        .unwrap();

    let (_, v_map_stale) =
        send_json(&h.app, Method::GET, "/api/v1/corridors/map", None, None).await;
    assert!(v_map_stale["features"][0]["properties"]["ivv_score"].is_null());
    assert_eq!(
        v_map_stale["features"][0]["properties"]["risk_level"],
        "UNKNOWN"
    );
    assert_eq!(v_map_stale["features"][0]["properties"]["is_stale"], true);

    let (st_rte, v_rte) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/routing/calculate",
        Some(&coer_tok),
        Some(json!({
            "origin_node_id": n1,
            "destination_node_id": n2,
            "vehicle_category_code": "MOTORCYCLE",
            "cargo_type": "MEDICAL_SUPPLIES",
            "cargo_payload_kg": 10
        })),
    )
    .await;
    assert_eq!(st_rte, StatusCode::OK);
    assert_eq!(v_rte["resolution_type"], "NO_SAFE_ROUTE");
    assert_eq!(v_rte["reason_code"], "STALE_IVV");
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_4_con_01_consensus_deduplication_and_margin() {
    let h = setup_harness().await;
    let d1 = seed_user(&h.pool, "DRIVER", "+51900000001", "Pass1234!").await;
    let _d2 = seed_user(&h.pool, "DRIVER", "+51900000002", "Pass1234!").await;
    let _civil = seed_user(&h.pool, "CIVIL_DEFENSE", "+51900000005", "Pass1234!").await;
    let _health = seed_user(&h.pool, "HEALTH_OPERATOR", "+51900000004", "Pass1234!").await;

    let (t1, _) = login_password(&h.app, "+51900000001", "Pass1234!", "dev1").await;
    let (t2, _) = login_password(&h.app, "+51900000002", "Pass1234!", "dev2").await;
    let (tciv, _) = login_password(&h.app, "+51900000005", "Pass1234!", "dev3").await;
    let (thlt, _) = login_password(&h.app, "+51900000004", "Pass1234!", "dev4").await;

    sqlx::query("UPDATE users SET reputation_score = 0.50 WHERE user_id = $1")
        .bind(d1)
        .execute(&h.pool)
        .await
        .unwrap();

    let n1 = Uuid::new_v4();
    let n2 = Uuid::new_v4();
    let seg = Uuid::new_v4();
    insert_node(&h.pool, n1, "A", -76.000, -9.900, 0).await;
    insert_node(&h.pool, n2, "B", -75.991, -9.900, 0).await;
    insert_segment(
        &h.pool,
        seg,
        n1,
        n2,
        "LINESTRING(-76.000 -9.900, -75.991 -9.900)",
        "6.0",
        Some("6.0"),
        true,
    )
    .await;

    // 1 user sends 5 BLOCKED reports -> deduplicated -> remains SUSPECTED
    for i in 0..5 {
        let (st, _) = send_json(
            &h.app,
            Method::POST,
            "/api/v1/reports",
            Some(&t1),
            Some(json!({
                "client_uuid": Uuid::new_v4(),
                "segment_id": seg,
                "proposed_passability": "BLOCKED",
                "latitude": -9.900,
                "longitude": -75.995,
                "captured_at_client": (Utc::now() - Duration::seconds(60 - i)).to_rfc3339()
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
    }

    let state1: String =
        sqlx::query_scalar("SELECT verification_state FROM incidents WHERE segment_id = $1")
            .bind(seg)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(state1, "SUSPECTED");

    // Second driver votes BLOCKED; strong opposer votes OPEN -> margin < 0.50 -> stays SUSPECTED
    let _ = send_json(
        &h.app,
        Method::POST,
        "/api/v1/reports",
        Some(&t2),
        Some(json!({
            "client_uuid": Uuid::new_v4(),
            "segment_id": seg,
            "proposed_passability": "BLOCKED",
            "latitude": -9.900,
            "longitude": -75.995,
            "captured_at_client": Utc::now().to_rfc3339()
        })),
    )
    .await;
    let _ = send_json(
        &h.app,
        Method::POST,
        "/api/v1/reports",
        Some(&tciv),
        Some(json!({
            "client_uuid": Uuid::new_v4(),
            "segment_id": seg,
            "proposed_passability": "OPEN",
            "latitude": -9.900,
            "longitude": -75.995,
            "captured_at_client": Utc::now().to_rfc3339()
        })),
    )
    .await;
    let state2: String =
        sqlx::query_scalar("SELECT verification_state FROM incidents WHERE segment_id = $1")
            .bind(seg)
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(state2, "SUSPECTED");

    // Health operator breaks tie margin >= 0.50 -> upgrades incident to CONFIRMED BLOCKED
    let _ = send_json(
        &h.app,
        Method::POST,
        "/api/v1/reports",
        Some(&thlt),
        Some(json!({
            "client_uuid": Uuid::new_v4(),
            "segment_id": seg,
            "proposed_passability": "BLOCKED",
            "latitude": -9.900,
            "longitude": -75.995,
            "captured_at_client": Utc::now().to_rfc3339()
        })),
    )
    .await;
    let row3 = sqlx::query(
        "SELECT verification_state, impact_passability FROM incidents WHERE segment_id = $1",
    )
    .bind(seg)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(
        row3.try_get::<String, _>("verification_state").unwrap(),
        "CONFIRMED"
    );
    assert_eq!(
        row3.try_get::<String, _>("impact_passability").unwrap(),
        "BLOCKED"
    );
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_5_rte_brg_windows_and_bridge_authorization() {
    let h = setup_harness().await;
    let _health_id = seed_user(&h.pool, "HEALTH_OPERATOR", "+51900000004", "Pass1234!").await;
    let _civil_id = seed_user(&h.pool, "CIVIL_DEFENSE", "+51900000005", "Pass1234!").await;
    let (thlt, _) = login_password(&h.app, "+51900000004", "Pass1234!", "dev-h").await;
    let (tciv, _) = login_password(&h.app, "+51900000005", "Pass1234!", "dev-c").await;

    let orig = Uuid::new_v4();
    let side_a = Uuid::new_v4();
    let side_b = Uuid::new_v4();
    let dest = Uuid::new_v4();
    insert_node(&h.pool, orig, "Orig", -76.020, -9.710, 0).await;
    insert_node(&h.pool, side_a, "SideA", -76.010, -9.710, 0).await;
    insert_node(&h.pool, side_b, "SideB", -76.000, -9.710, 0).await;
    insert_node(&h.pool, dest, "Dest", -75.990, -9.710, 0).await;

    let s1 = Uuid::new_v4();
    let s_mid = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    insert_segment(
        &h.pool,
        s1,
        orig,
        side_a,
        "LINESTRING(-76.020 -9.710, -76.010 -9.710)",
        "6.0",
        Some("14.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        s_mid,
        side_a,
        side_b,
        "LINESTRING(-76.010 -9.710, -76.000 -9.710)",
        "6.0",
        Some("6.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        s2,
        side_b,
        dest,
        "LINESTRING(-76.000 -9.710, -75.990 -9.710)",
        "6.0",
        Some("6.0"),
        true,
    )
    .await;
    attach_fresh_ivv(&h.pool, s1, 5.0, 10.0, "20.00").await;
    attach_fresh_ivv(&h.pool, s_mid, 5.0, 10.0, "20.00").await;
    attach_fresh_ivv(&h.pool, s2, 5.0, 10.0, "20.00").await;

    // RTE-02: DEST_TO_ORIGIN has 14% slope -> VEHICLE_RESTRICTION for HEAVY_TRAILER
    let (st_rev, v_rev) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/routing/calculate",
        Some(&thlt),
        Some(json!({
            "origin_node_id": side_a,
            "destination_node_id": orig,
            "vehicle_category_code": "HEAVY_TRAILER",
            "cargo_type": "MEDICAL_SUPPLIES",
            "cargo_payload_kg": 100
        })),
    )
    .await;
    assert_eq!(st_rev, StatusCode::OK);
    assert_eq!(v_rev["resolution_type"], "NO_SAFE_ROUTE");
    assert_eq!(v_rev["reason_code"], "VEHICLE_RESTRICTION");

    // Block middle segment via confirmed incident
    let inc_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents (incident_id, segment_id, trigger_source, verification_state, impact_passability, hazard_location_geom, hazard_radius_m, confirmed_at)
         VALUES ($1, $2, 'AUTHORITY_DIRECT', 'CONFIRMED', 'BLOCKED', ST_SetSRID(ST_MakePoint(-76.005, -9.710), 4326), 150, NOW())",
    )
    .bind(inc_id)
    .bind(s_mid)
    .execute(&h.pool)
    .await
    .unwrap();

    // Add verified transfer corridor offset south (outside 150m hazard)
    let corr_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO transfer_corridors (
            corridor_id, side_a_node_id, side_b_node_id, path_geom, length_m, max_slope_pct,
            transfer_mode, max_safe_ivv, is_verified_safe, last_inspected_at,
            inspection_valid_days, requires_revalidation, verified_by_user_id
         ) VALUES (
            $1, $2, $3, ST_GeomFromText('LINESTRING(-76.010 -9.715, -76.000 -9.715)', 4326),
            400, 10.0, 'PEDESTRIAN_SAFE_PATH', 60.00, TRUE, NOW(), 30, FALSE,
            (SELECT user_id FROM users WHERE phone_e164 = '+51900000005')
         )",
    )
    .bind(corr_id)
    .bind(side_a)
    .bind(side_b)
    .execute(&h.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO transfer_corridor_capabilities (corridor_id, cargo_type, max_payload_kg, requires_medical_supervision, estimated_transfer_time_min)
         VALUES ($1, 'PERISHABLE_AGRI', 1500.00, FALSE, 25.00)",
    )
    .bind(corr_id)
    .execute(&h.pool)
    .await
    .unwrap();

    // CTR-01 + Bridge discovery
    let (st_brg, v_brg) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/routing/calculate",
        Some(&thlt),
        Some(json!({
            "origin_node_id": orig,
            "destination_node_id": dest,
            "vehicle_category_code": "HEAVY_TRAILER",
            "cargo_type": "PERISHABLE_AGRI",
            "cargo_payload_kg": 1200
        })),
    )
    .await;
    assert_eq!(st_brg, StatusCode::OK);
    assert_eq!(v_brg["resolution_type"], "LOGISTIC_BRIDGE_REQUIRED");
    assert_eq!(
        v_brg["bridge_proposal"]["corridor_id"].as_str().unwrap(),
        corr_id.to_string()
    );

    // HEALTH_OPERATOR proposes bridge (PROPOSED), cannot activate unilaterally (403); CIVIL_DEFENSE activates (200)
    let (st_prop, v_prop) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/bridges",
        Some(&thlt),
        Some(json!({
            "incident_id": inc_id,
            "corridor_id": corr_id,
            "cargo_type": "PERISHABLE_AGRI"
        })),
    )
    .await;
    assert_eq!(st_prop, StatusCode::CREATED);
    assert_eq!(v_prop["status"], "PROPOSED");
    let bridge_id = v_prop["bridge_id"].as_str().unwrap().to_string();

    let (st_act_forbid, _) = send_json(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/bridges/{bridge_id}/status"),
        Some(&thlt),
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(st_act_forbid, StatusCode::FORBIDDEN);

    let (st_act_ok, v_act_ok) = send_json(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/bridges/{bridge_id}/status"),
        Some(&tciv),
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(st_act_ok, StatusCode::OK);
    assert_eq!(v_act_ok["status"], "ACTIVE");
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_6_syn_concurrency_lock_pagination_and_idempotency() {
    let h = setup_harness().await;
    seed_user(&h.pool, "DRIVER", "+51900000001", "Pass1234!").await;
    seed_user(&h.pool, "DRIVER", "+51900000002", "Pass1234!").await;
    let (t1, _) = login_password(&h.app, "+51900000001", "Pass1234!", "syn1").await;
    let (t2, _) = login_password(&h.app, "+51900000002", "Pass1234!", "syn2").await;

    // SYN-02: Transactional lock serialization on sync_control
    let pool_a = h.pool.clone();
    let pool_b = h.pool.clone();
    let mut tx_a = pool_a.begin().await.unwrap();
    let seq_a = append_sync_change(
        &mut tx_a,
        "ROAD_NODE",
        Uuid::new_v4(),
        "UPSERT",
        Some(json!({"step": "A"})),
    )
    .await
    .unwrap();
    assert_eq!(seq_a, 1);

    let task_b = tokio::spawn(async move {
        let mut tx_b = pool_b.begin().await.unwrap();
        let seq_b = append_sync_change(
            &mut tx_b,
            "ROAD_NODE",
            Uuid::new_v4(),
            "UPSERT",
            Some(json!({"step": "B"})),
        )
        .await
        .unwrap();
        tx_b.commit().await.unwrap();
        seq_b
    });

    tokio::time::sleep(StdDuration::from_millis(120)).await;
    // Before tx_a commits, neither seq 1 nor seq 2 is visible in /sync/snapshot!
    let (st_mid, v_mid) = send_json(
        &h.app,
        Method::GET,
        "/api/v1/sync/snapshot?since_version=0&limit=10",
        Some(&t1),
        None,
    )
    .await;
    assert_eq!(st_mid, StatusCode::OK);
    assert_eq!(v_mid["changes"].as_array().unwrap().len(), 0);
    assert_eq!(v_mid["next_since_version"], 0);

    tx_a.commit().await.unwrap();
    let seq_b = task_b.await.unwrap();
    assert_eq!(seq_b, 2);

    // SYN-03: Paginated sequential cursor limit=1
    let (_, p1) = send_json(
        &h.app,
        Method::GET,
        "/api/v1/sync/snapshot?since_version=0&limit=1",
        Some(&t1),
        None,
    )
    .await;
    assert_eq!(p1["changes"].as_array().unwrap().len(), 1);
    assert_eq!(p1["changes"][0]["change_seq"], 1);
    assert_eq!(p1["has_more"], true);
    assert_eq!(p1["next_since_version"], 1);

    let (_, p2) = send_json(
        &h.app,
        Method::GET,
        "/api/v1/sync/snapshot?since_version=1&limit=1",
        Some(&t1),
        None,
    )
    .await;
    assert_eq!(p2["changes"].as_array().unwrap().len(), 1);
    assert_eq!(p2["changes"][0]["change_seq"], 2);
    assert_eq!(p2["has_more"], false);

    // SYN-04: Batch idempotency + cross-user 409 conflict
    let n1 = Uuid::new_v4();
    let n2 = Uuid::new_v4();
    let seg = Uuid::new_v4();
    insert_node(&h.pool, n1, "SN1", -76.000, -9.900, 0).await;
    insert_node(&h.pool, n2, "SN2", -75.991, -9.900, 0).await;
    insert_segment(
        &h.pool,
        seg,
        n1,
        n2,
        "LINESTRING(-76.000 -9.900, -75.991 -9.900)",
        "5.0",
        Some("5.0"),
        true,
    )
    .await;

    let client_uuid = Uuid::new_v4();
    let batch_body = json!({
        "reports": [{
            "client_uuid": client_uuid,
            "segment_id": seg,
            "proposed_passability": "BLOCKED",
            "latitude": -9.900,
            "longitude": -75.995,
            "captured_at_client": Utc::now().to_rfc3339()
        }],
        "telemetry_batches": []
    });
    let (st_b1, v_b1) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/sync/batch",
        Some(&t1),
        Some(batch_body.clone()),
    )
    .await;
    assert_eq!(st_b1, StatusCode::OK);
    assert_eq!(v_b1["report_results"][0]["status"], "CREATED");

    let (st_b2, v_b2) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/sync/batch",
        Some(&t1),
        Some(batch_body.clone()),
    )
    .await;
    assert_eq!(st_b2, StatusCode::OK);
    assert_eq!(v_b2["report_results"][0]["status"], "DUPLICATE_IGNORED");

    let (st_conflict, v_conflict) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/sync/batch",
        Some(&t2),
        Some(batch_body),
    )
    .await;
    assert_eq!(st_conflict, StatusCode::CONFLICT);
    assert_eq!(v_conflict["error_code"], "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
#[ignore = "requires live PostGIS DATABASE_URL"]
async fn test_7_sim_imp_e2e_01_huanuco_carpish_drill_isolation() {
    let h = setup_harness().await;
    let _coer_id = seed_user(&h.pool, "COER_ADMIN", "+51900000006", "PassCoer123!").await;
    let (tcoer, _) = login_password(&h.app, "+51900000006", "PassCoer123!", "coer-e2e").await;

    // Seed drill road topology nodes/segments (NEVER inserting SIM_TRANSFER_CORRIDOR_ID into real transfer_corridors!).
    insert_node(&h.pool, NODE_HUANUCO_ID, "Huanuco", -76.242, -9.930, 0).await;
    insert_node(&h.pool, NODE_AMBO_ID, "Ambo", -76.205, -10.130, 0).await;
    insert_node(
        &h.pool,
        NODE_CARPISH_SUR_ID,
        "Carpish Sur",
        -76.091,
        -9.715,
        0,
    )
    .await;
    insert_node(
        &h.pool,
        NODE_CARPISH_NORTE_ID,
        "Carpish Norte",
        -76.091,
        -9.705,
        0,
    )
    .await;
    insert_node(
        &h.pool,
        NODE_TINGO_MARIA_ID,
        "Tingo Maria",
        -75.999,
        -9.295,
        0,
    )
    .await;

    insert_segment(
        &h.pool,
        SEG_HUANUCO_CARPISH_SUR_ID,
        NODE_HUANUCO_ID,
        NODE_CARPISH_SUR_ID,
        "LINESTRING(-76.242 -9.930, -76.091 -9.715)",
        "7.5",
        Some("7.5"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        SEG_CARPISH_TUNNEL_ID,
        NODE_CARPISH_SUR_ID,
        NODE_CARPISH_NORTE_ID,
        "LINESTRING(-76.091 -9.715, -76.091 -9.705)",
        "27.0",
        Some("27.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        SEG_CARPISH_NORTE_TINGO_ID,
        NODE_CARPISH_NORTE_ID,
        NODE_TINGO_MARIA_ID,
        "LINESTRING(-76.091 -9.705, -75.999 -9.295)",
        "8.0",
        Some("8.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        SEG_BYPASS_AMBO_ID,
        NODE_HUANUCO_ID,
        NODE_AMBO_ID,
        "LINESTRING(-76.242 -9.930, -76.205 -10.130)",
        "18.0",
        Some("18.0"),
        true,
    )
    .await;
    insert_segment(
        &h.pool,
        SEG_BYPASS_TINGO_ID,
        NODE_AMBO_ID,
        NODE_TINGO_MARIA_ID,
        "LINESTRING(-76.205 -10.130, -75.999 -9.295)",
        "19.0",
        Some("19.0"),
        true,
    )
    .await;

    // SIM-05 DB check: sim_pool MUST be denied write access on operational tables
    let denied = sqlx::query("DELETE FROM roles").execute(&h.sim_pool).await;
    assert!(denied.is_err());

    let started = Instant::now();

    // 1. Create scenario
    let (st_sc, v_sc) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/simulator/scenarios",
        Some(&tcoer),
        Some(json!({
            "name": "Simulacro Lluvia Extrema Carpish",
            "fixture_tag": DRILL_FIXTURE_TAG
        })),
    )
    .await;
    assert_eq!(st_sc, StatusCode::CREATED);
    let sc_id = v_sc["scenario_id"].as_str().unwrap().to_string();

    // SIM-04: Reject mixed event variant fields with 422
    let (st_bad_ev, _) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/events"),
        Some(&tcoer),
        Some(json!({
            "segment_id": SEG_CARPISH_TUNNEL_ID,
            "event_type": "WEATHER_OVERRIDE",
            "precip_24h_mm": 85.0,
            "precip_72h_mm": 190.0,
            "simulated_passability": "BLOCKED"
        })),
    )
    .await;
    assert_eq!(st_bad_ev, StatusCode::UNPROCESSABLE_ENTITY);

    // 2. Inject rain (rev 1) + Carpish landslide blockage (rev 2)
    let (st_w, v_w) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/events"),
        Some(&tcoer),
        Some(json!({
            "segment_id": SEG_CARPISH_TUNNEL_ID,
            "event_type": "WEATHER_OVERRIDE",
            "precip_24h_mm": 85.0,
            "precip_72h_mm": 190.0
        })),
    )
    .await;
    assert_eq!(st_w, StatusCode::CREATED);
    assert_eq!(v_w["execution_context"], "SIMULATION");

    let (st_b, v_b) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/events"),
        Some(&tcoer),
        Some(json!({
            "segment_id": SEG_CARPISH_TUNNEL_ID,
            "event_type": "BLOCKAGE_OVERRIDE",
            "simulated_passability": "BLOCKED",
            "hazard_lat": -9.712,
            "hazard_lon": -76.091,
            "hazard_radius_m": 200
        })),
    )
    .await;
    assert_eq!(st_b, StatusCode::CREATED);
    assert_eq!(v_b["execution_context"], "SIMULATION");
    assert!(v_b["simulated_notifications"][0]["preview"]
        .as_str()
        .unwrap()
        .starts_with("[SIMULATION]"));

    let eval_time = "2026-11-20T09:00:00Z";

    // 3a. AMBULANCE_4X4 gets ALTERNATE_ROUTE via Ambo bypass
    let (st_amb, v_amb) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/routing"),
        Some(&tcoer),
        Some(json!({
            "origin_node_id": NODE_HUANUCO_ID,
            "destination_node_id": NODE_TINGO_MARIA_ID,
            "vehicle_category_code": "AMBULANCE_4X4",
            "cargo_type": "MEDICAL_PATIENT",
            "patient_count": 1,
            "scenario_revision": 2,
            "evaluation_time": eval_time
        })),
    )
    .await;
    assert_eq!(st_amb, StatusCode::OK);
    assert_eq!(v_amb["execution_context"], "SIMULATION");
    assert_eq!(v_amb["resolution_type"], "ALTERNATE_ROUTE");

    // 3b. HEAVY_TRAILER gets LOGISTIC_BRIDGE_REQUIRED via isolated drill corridor
    let (st_trk, v_trk) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/routing"),
        Some(&tcoer),
        Some(json!({
            "origin_node_id": NODE_HUANUCO_ID,
            "destination_node_id": NODE_TINGO_MARIA_ID,
            "vehicle_category_code": "HEAVY_TRAILER",
            "cargo_type": "PERISHABLE_AGRI",
            "cargo_payload_kg": 1200,
            "scenario_revision": 2,
            "evaluation_time": eval_time
        })),
    )
    .await;
    assert_eq!(st_trk, StatusCode::OK);
    assert_eq!(v_trk["resolution_type"], "LOGISTIC_BRIDGE_REQUIRED");
    assert_eq!(
        v_trk["bridge_proposal"]["corridor_id"].as_str().unwrap(),
        SIM_TRANSFER_CORRIDOR_ID.to_string()
    );

    // SIM-06: Production /bridges MUST reject synthetic drill transfer corridor ID
    let dummy_inc = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents (incident_id, segment_id, trigger_source, verification_state, impact_passability, hazard_location_geom, confirmed_at)
         VALUES ($1, $2, 'AUTHORITY_DIRECT', 'CONFIRMED', 'BLOCKED', ST_SetSRID(ST_MakePoint(-76.091, -9.712), 4326), NOW())",
    )
    .bind(dummy_inc)
    .bind(SEG_CARPISH_TUNNEL_ID)
    .execute(&h.pool)
    .await
    .unwrap();
    let (st_syn_brg, _) = send_json(
        &h.app,
        Method::POST,
        "/api/v1/bridges",
        Some(&tcoer),
        Some(json!({
            "incident_id": dummy_inc,
            "corridor_id": SIM_TRANSFER_CORRIDOR_ID,
            "cargo_type": "PERISHABLE_AGRI"
        })),
    )
    .await;
    assert_eq!(st_syn_brg, StatusCode::UNPROCESSABLE_ENTITY);

    // 4a. Evaluate AMBULANCE_4X4 long detour -> negative CO2 delta preserved (SIM-02)
    let (st_eval1, _) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/evaluate"),
        Some(&tcoer),
        Some(json!({
            "origin_node_id": NODE_HUANUCO_ID,
            "destination_node_id": NODE_TINGO_MARIA_ID,
            "vehicle_category_code": "AMBULANCE_4X4",
            "cargo_type": "MEDICAL_PATIENT",
            "patient_count": 1,
            "baseline_wait_min": 30,
            "scenario_revision": 2,
            "evaluation_time": eval_time
        })),
    )
    .await;
    assert_eq!(st_eval1, StatusCode::CREATED);

    // 4b. Evaluate overloaded HEAVY_TRAILER -> NO_SAFE_ROUTE -> NULL mitigated & deltas (SIM-01 / IMP-01)
    let (st_eval2, v_eval2) = send_json(
        &h.app,
        Method::POST,
        &format!("/api/v1/simulator/scenarios/{sc_id}/evaluate"),
        Some(&tcoer),
        Some(json!({
            "origin_node_id": NODE_HUANUCO_ID,
            "destination_node_id": NODE_TINGO_MARIA_ID,
            "vehicle_category_code": "HEAVY_TRAILER",
            "cargo_type": "PERISHABLE_AGRI",
            "cargo_payload_kg": 2500,
            "baseline_wait_min": 360,
            "scenario_revision": 2,
            "evaluation_time": eval_time
        })),
    )
    .await;
    assert_eq!(st_eval2, StatusCode::CREATED);
    assert_eq!(v_eval2["resolution_type"], "NO_SAFE_ROUTE");
    assert_eq!(v_eval2["mitigation_status"], "UNMITIGATED_NO_SAFE_ROUTE");

    let (st_imp, v_imp) = send_json(
        &h.app,
        Method::GET,
        &format!("/api/v1/simulator/scenarios/{sc_id}/impact"),
        Some(&tcoer),
        None,
    )
    .await;
    assert_eq!(st_imp, StatusCode::OK);
    let evals = v_imp["evaluations"].as_array().unwrap();
    assert_eq!(evals.len(), 2);
    assert_eq!(evals[0]["resolution_type"], "ALTERNATE_ROUTE");
    assert!(evals[0]["deltas"]["net_co2_delta_kg"].as_f64().unwrap() < 0.0);
    assert_eq!(evals[1]["resolution_type"], "NO_SAFE_ROUTE");
    assert!(evals[1]["mitigated"]["travel_min"].is_null());
    assert!(evals[1]["deltas"]["time_delta_min"].is_null());
    assert!(evals[1]["deltas"]["net_co2_delta_kg"].is_null());

    // Confirm production transfer_corridors, logistic_bridges, and sync_control remained untouched
    let tc_count: i64 = sqlx::query_scalar("SELECT count(*) FROM transfer_corridors")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    let brg_count: i64 = sqlx::query_scalar("SELECT count(*) FROM logistic_bridges")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    let sync_seq: i64 =
        sqlx::query_scalar("SELECT last_seq FROM sync_control WHERE singleton_id = 1")
            .fetch_one(&h.pool)
            .await
            .unwrap();
    assert_eq!(tc_count, 0);
    assert_eq!(brg_count, 0);
    assert_eq!(sync_seq, 0);

    // Performance budget: entire E2E contingency drill << 2 seconds
    assert!(
        started.elapsed() < StdDuration::from_secs(2),
        "E2E drill exceeded 2s budget: {:?}",
        started.elapsed()
    );

    // Also keep RoleCode & encode_access_token check referenced cleanly
    let _tok = encode_access_token(
        Uuid::new_v4(),
        RoleCode::CoerAdmin,
        h.config.jwt_secret.expose_secret().as_bytes(),
        15,
    )
    .unwrap();
}
