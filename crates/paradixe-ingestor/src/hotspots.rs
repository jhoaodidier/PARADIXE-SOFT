#![forbid(unsafe_code)]

use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::regions::RegionSpec;

#[derive(Debug, Clone, Copy)]
pub struct GeotechnicalHotspotZone {
    pub name: &'static str,
    pub lon: f64,
    pub lat: f64,
    pub radius_m: f64,
}

/// Catalog of documented INGEMMET / Provias critical slope & huaico zones across the Macro-Center corridor.
pub const ANDEAN_CRITICAL_ZONES: &[GeotechnicalHotspotZone] = &[
    GeotechnicalHotspotZone {
        name: "Tunel y Paso Carpish",
        lon: -76.0910,
        lat: -9.7120,
        radius_m: 4500.0,
    },
    GeotechnicalHotspotZone {
        name: "Chinchao - Puente Durand",
        lon: -76.0400,
        lat: -9.6500,
        radius_m: 3500.0,
    },
    GeotechnicalHotspotZone {
        name: "Mirador - Acomayo",
        lon: -76.0850,
        lat: -9.7800,
        radius_m: 3000.0,
    },
    GeotechnicalHotspotZone {
        name: "Ambo - San Rafael",
        lon: -76.1900,
        lat: -10.2200,
        radius_m: 3500.0,
    },
    GeotechnicalHotspotZone {
        name: "Boqueron del Padre Abad",
        lon: -75.6700,
        lat: -9.1200,
        radius_m: 5000.0,
    },
    GeotechnicalHotspotZone {
        name: "Aguaytia - La Divisoria",
        lon: -75.7900,
        lat: -9.1800,
        radius_m: 4000.0,
    },
    GeotechnicalHotspotZone {
        name: "La Oroya - Tarma",
        lon: -75.7950,
        lat: -11.4550,
        radius_m: 4000.0,
    },
    GeotechnicalHotspotZone {
        name: "San Ramon - La Merced",
        lon: -75.3400,
        lat: -11.1200,
        radius_m: 4000.0,
    },
    GeotechnicalHotspotZone {
        name: "Oxapampa - Huancabamba",
        lon: -75.4600,
        lat: -10.5050,
        radius_m: 3500.0,
    },
    GeotechnicalHotspotZone {
        name: "Cerro de Pasco - Yanahuanca",
        lon: -76.3900,
        lat: -10.5600,
        radius_m: 3500.0,
    },
];

#[derive(Debug, Clone, Default, Serialize)]
pub struct HotspotSyncStats {
    pub zones_applied: usize,
    pub segments_marked: usize,
}

pub async fn sync_historical_hotspots(
    pool: &PgPool,
    region: Option<&RegionSpec>,
) -> Result<HotspotSyncStats, String> {
    let zones: Vec<&GeotechnicalHotspotZone> = match region {
        Some(reg) => {
            let padded = reg.bbox.padded(0.10);
            ANDEAN_CRITICAL_ZONES
                .iter()
                .filter(|z| padded.contains(z.lon, z.lat))
                .collect()
        }
        None => ANDEAN_CRITICAL_ZONES.iter().collect(),
    };

    if zones.is_empty() {
        return Ok(HotspotSyncStats::default());
    }

    let lons: Vec<f64> = zones.iter().map(|z| z.lon).collect();
    let lats: Vec<f64> = zones.iter().map(|z| z.lat).collect();
    let radii: Vec<f64> = zones.iter().map(|z| z.radius_m).collect();

    let updated = if let Some(reg) = region {
        sqlx::query(
            "UPDATE road_segments rs
             SET is_historical_hotspot = TRUE,
                 soil_susceptibility_idx = GREATEST(rs.soil_susceptibility_idx, 0.72)
             WHERE rs.is_topology_verified = TRUE
               AND ST_Intersects(rs.geom, ST_GeomFromText($4, 4326))
               AND EXISTS (
                   SELECT 1
                   FROM UNNEST($1::float8[], $2::float8[], $3::float8[]) AS z(lon, lat, rad_m)
                   WHERE ST_DWithin(
                       rs.geom::geography,
                       ST_SetSRID(ST_MakePoint(z.lon, z.lat), 4326)::geography,
                       z.rad_m
                   )
               )",
        )
        .bind(&lons)
        .bind(&lats)
        .bind(&radii)
        .bind(reg.bbox.padded(0.05).polygon_wkt())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query(
            "UPDATE road_segments rs
             SET is_historical_hotspot = TRUE,
                 soil_susceptibility_idx = GREATEST(rs.soil_susceptibility_idx, 0.72)
             WHERE rs.is_topology_verified = TRUE
               AND EXISTS (
                   SELECT 1
                   FROM UNNEST($1::float8[], $2::float8[], $3::float8[]) AS z(lon, lat, rad_m)
                   WHERE ST_DWithin(
                       rs.geom::geography,
                       ST_SetSRID(ST_MakePoint(z.lon, z.lat), 4326)::geography,
                       z.rad_m
                   )
               )",
        )
        .bind(&lons)
        .bind(&lats)
        .bind(&radii)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
    };

    let _cnt_check = sqlx::query(
        "SELECT count(*)::bigint AS c FROM road_segments WHERE is_historical_hotspot = TRUE",
    )
    .fetch_one(pool)
    .await
    .ok()
    .and_then(|r| r.try_get::<i64, _>("c").ok())
    .unwrap_or(0);

    Ok(HotspotSyncStats {
        zones_applied: zones.len(),
        segments_marked: usize::try_from(updated.rows_affected()).unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_carpish_and_macro_center_coordinates() {
        assert!(ANDEAN_CRITICAL_ZONES.len() >= 10);
        for z in ANDEAN_CRITICAL_ZONES {
            assert!((-82.0..=-68.0).contains(&z.lon));
            assert!((-19.0..=0.0).contains(&z.lat));
            assert!(z.radius_m >= 1000.0);
        }
    }
}
