#![forbid(unsafe_code)]

use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl BBox {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return Err("bbox must be 'min_lon,min_lat,max_lon,max_lat'".to_string());
        }
        let min_lon: f64 = parts[0].parse().map_err(|_| "invalid min_lon")?;
        let min_lat: f64 = parts[1].parse().map_err(|_| "invalid min_lat")?;
        let max_lon: f64 = parts[2].parse().map_err(|_| "invalid max_lon")?;
        let max_lat: f64 = parts[3].parse().map_err(|_| "invalid max_lat")?;
        if !(-180.0..=180.0).contains(&min_lon)
            || !(-180.0..=180.0).contains(&max_lon)
            || !(-90.0..=90.0).contains(&min_lat)
            || !(-90.0..=90.0).contains(&max_lat)
            || min_lon >= max_lon
            || min_lat >= max_lat
        {
            return Err("bbox bounds must satisfy min < max in WGS84".to_string());
        }
        Ok(Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        })
    }

    pub fn padded(&self, pad_deg: f64) -> Self {
        Self {
            min_lon: (self.min_lon - pad_deg).max(-180.0),
            min_lat: (self.min_lat - pad_deg).max(-85.0511),
            max_lon: (self.max_lon + pad_deg).min(180.0),
            max_lat: (self.max_lat + pad_deg).min(85.0511),
        }
    }

    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        lon >= self.min_lon && lon <= self.max_lon && lat >= self.min_lat && lat <= self.max_lat
    }

    pub fn center(&self) -> (f64, f64) {
        (
            (self.min_lon + self.max_lon) / 2.0,
            (self.min_lat + self.max_lat) / 2.0,
        )
    }

    pub fn polygon_wkt(&self) -> String {
        format!(
            "POLYGON(({} {}, {} {}, {} {}, {} {}, {} {}))",
            self.min_lon,
            self.min_lat,
            self.max_lon,
            self.min_lat,
            self.max_lon,
            self.max_lat,
            self.min_lon,
            self.max_lat,
            self.min_lon,
            self.min_lat
        )
    }
}

#[derive(Debug, Clone)]
pub struct ProvinceSpec {
    pub ubigeo_code: &'static str,
    pub name: &'static str,
    pub bbox: BBox,
    pub capital_lon: f64,
    pub capital_lat: f64,
}

#[derive(Debug, Clone)]
pub struct RegionSpec {
    pub iso_code: String,
    pub ubigeo_code: String,
    pub name: String,
    pub slug: String,
    pub bbox: BBox,
    pub center_zoom: u8,
    pub provinces: Vec<ProvinceSpec>,
}

const HUANUCO_PROVINCES: &[ProvinceSpec] = &[
    ProvinceSpec {
        ubigeo_code: "1001",
        name: "Huanuco",
        bbox: BBox {
            min_lon: -76.55,
            min_lat: -10.12,
            max_lon: -75.78,
            max_lat: -9.55,
        },
        capital_lon: -76.2422,
        capital_lat: -9.9306,
    },
    ProvinceSpec {
        ubigeo_code: "1002",
        name: "Ambo",
        bbox: BBox {
            min_lon: -76.48,
            min_lat: -10.38,
            max_lon: -75.95,
            max_lat: -10.02,
        },
        capital_lon: -76.2053,
        capital_lat: -10.1306,
    },
    ProvinceSpec {
        ubigeo_code: "1003",
        name: "Dos de Mayo",
        bbox: BBox {
            min_lon: -76.98,
            min_lat: -9.96,
            max_lon: -76.42,
            max_lat: -9.45,
        },
        capital_lon: -76.8019,
        capital_lat: -9.8300,
    },
    ProvinceSpec {
        ubigeo_code: "1004",
        name: "Huacaybamba",
        bbox: BBox {
            min_lon: -77.15,
            min_lat: -9.20,
            max_lon: -76.55,
            max_lat: -8.70,
        },
        capital_lon: -76.9522,
        capital_lat: -9.0358,
    },
    ProvinceSpec {
        ubigeo_code: "1005",
        name: "Huamalies",
        bbox: BBox {
            min_lon: -77.05,
            min_lat: -9.70,
            max_lon: -76.10,
            max_lat: -9.15,
        },
        capital_lon: -76.8167,
        capital_lat: -9.5494,
    },
    ProvinceSpec {
        ubigeo_code: "1006",
        name: "Leoncio Prado",
        bbox: BBox {
            min_lon: -76.35,
            min_lat: -9.60,
            max_lon: -75.60,
            max_lat: -8.65,
        },
        capital_lon: -75.9997,
        capital_lat: -9.2953,
    },
    ProvinceSpec {
        ubigeo_code: "1007",
        name: "Maranon",
        bbox: BBox {
            min_lon: -77.35,
            min_lat: -8.90,
            max_lon: -76.00,
            max_lat: -8.35,
        },
        capital_lon: -77.1494,
        capital_lat: -8.5989,
    },
    ProvinceSpec {
        ubigeo_code: "1008",
        name: "Pachitea",
        bbox: BBox {
            min_lon: -76.15,
            min_lat: -10.35,
            max_lon: -75.40,
            max_lat: -9.70,
        },
        capital_lon: -75.9736,
        capital_lat: -9.9850,
    },
    ProvinceSpec {
        ubigeo_code: "1009",
        name: "Puerto Inca",
        bbox: BBox {
            min_lon: -75.60,
            min_lat: -10.55,
            max_lon: -74.55,
            max_lat: -8.95,
        },
        capital_lon: -74.9669,
        capital_lat: -9.3778,
    },
    ProvinceSpec {
        ubigeo_code: "1010",
        name: "Lauricocha",
        bbox: BBox {
            min_lon: -76.95,
            min_lat: -10.45,
            max_lon: -76.45,
            max_lat: -9.90,
        },
        capital_lon: -76.6258,
        capital_lat: -10.0772,
    },
    ProvinceSpec {
        ubigeo_code: "1011",
        name: "Yarowilca",
        bbox: BBox {
            min_lon: -76.72,
            min_lat: -10.02,
            max_lon: -76.35,
            max_lat: -9.62,
        },
        capital_lon: -76.5978,
        capital_lat: -9.7969,
    },
];

const PASCO_PROVINCES: &[ProvinceSpec] = &[
    ProvinceSpec {
        ubigeo_code: "1901",
        name: "Pasco",
        bbox: BBox {
            min_lon: -76.70,
            min_lat: -11.15,
            max_lon: -75.60,
            max_lat: -10.40,
        },
        capital_lon: -76.2561,
        capital_lat: -10.6836,
    },
    ProvinceSpec {
        ubigeo_code: "1902",
        name: "Daniel Alcides Carrion",
        bbox: BBox {
            min_lon: -76.68,
            min_lat: -10.72,
            max_lon: -76.12,
            max_lat: -10.22,
        },
        capital_lon: -76.5125,
        capital_lat: -10.4936,
    },
    ProvinceSpec {
        ubigeo_code: "1903",
        name: "Oxapampa",
        bbox: BBox {
            min_lon: -75.85,
            min_lat: -10.95,
            max_lon: -74.20,
            max_lat: -9.40,
        },
        capital_lon: -75.4017,
        capital_lat: -10.5775,
    },
];

const JUNIN_PROVINCES: &[ProvinceSpec] = &[
    ProvinceSpec {
        ubigeo_code: "1201",
        name: "Huancayo",
        bbox: BBox {
            min_lon: -75.55,
            min_lat: -12.68,
            max_lon: -74.75,
            max_lat: -11.80,
        },
        capital_lon: -75.2049,
        capital_lat: -12.0651,
    },
    ProvinceSpec {
        ubigeo_code: "1202",
        name: "Concepcion",
        bbox: BBox {
            min_lon: -75.55,
            min_lat: -12.05,
            max_lon: -74.70,
            max_lat: -11.40,
        },
        capital_lon: -75.3125,
        capital_lat: -11.9186,
    },
    ProvinceSpec {
        ubigeo_code: "1203",
        name: "Chanchamayo",
        bbox: BBox {
            min_lon: -75.65,
            min_lat: -11.35,
            max_lon: -74.85,
            max_lat: -10.65,
        },
        capital_lon: -75.3267,
        capital_lat: -11.0544,
    },
    ProvinceSpec {
        ubigeo_code: "1204",
        name: "Jauja",
        bbox: BBox {
            min_lon: -75.95,
            min_lat: -12.00,
            max_lon: -75.15,
            max_lat: -11.35,
        },
        capital_lon: -75.4967,
        capital_lat: -11.7758,
    },
    ProvinceSpec {
        ubigeo_code: "1205",
        name: "Junin",
        bbox: BBox {
            min_lon: -76.30,
            min_lat: -11.45,
            max_lon: -75.65,
            max_lat: -10.85,
        },
        capital_lon: -75.9925,
        capital_lat: -11.1586,
    },
    ProvinceSpec {
        ubigeo_code: "1206",
        name: "Satipo",
        bbox: BBox {
            min_lon: -75.05,
            min_lat: -12.35,
            max_lon: -73.40,
            max_lat: -10.80,
        },
        capital_lon: -74.6383,
        capital_lat: -11.2528,
    },
    ProvinceSpec {
        ubigeo_code: "1207",
        name: "Tarma",
        bbox: BBox {
            min_lon: -76.00,
            min_lat: -11.62,
            max_lon: -75.35,
            max_lat: -11.12,
        },
        capital_lon: -75.6908,
        capital_lat: -11.4189,
    },
    ProvinceSpec {
        ubigeo_code: "1208",
        name: "Yauli",
        bbox: BBox {
            min_lon: -76.50,
            min_lat: -11.95,
            max_lon: -75.80,
            max_lat: -11.20,
        },
        capital_lon: -75.9014,
        capital_lat: -11.5236,
    },
    ProvinceSpec {
        ubigeo_code: "1209",
        name: "Chupaca",
        bbox: BBox {
            min_lon: -75.75,
            min_lat: -12.40,
            max_lon: -75.20,
            max_lat: -11.95,
        },
        capital_lon: -75.2889,
        capital_lat: -12.0619,
    },
];

const UCAYALI_PROVINCES: &[ProvinceSpec] = &[
    ProvinceSpec {
        ubigeo_code: "2501",
        name: "Coronel Portillo",
        bbox: BBox {
            min_lon: -75.25,
            min_lat: -9.55,
            max_lon: -72.90,
            max_lat: -7.80,
        },
        capital_lon: -74.5539,
        capital_lat: -8.3792,
    },
    ProvinceSpec {
        ubigeo_code: "2502",
        name: "Atalaya",
        bbox: BBox {
            min_lon: -74.55,
            min_lat: -11.45,
            max_lon: -72.25,
            max_lat: -9.35,
        },
        capital_lon: -73.7536,
        capital_lat: -10.7294,
    },
    ProvinceSpec {
        ubigeo_code: "2503",
        name: "Padre Abad",
        bbox: BBox {
            min_lon: -75.95,
            min_lat: -9.45,
            max_lon: -74.75,
            max_lat: -7.30,
        },
        capital_lon: -75.5094,
        capital_lat: -9.0389,
    },
    ProvinceSpec {
        ubigeo_code: "2504",
        name: "Purus",
        bbox: BBox {
            min_lon: -72.55,
            min_lat: -11.20,
            max_lon: -70.45,
            max_lat: -9.35,
        },
        capital_lon: -70.7069,
        capital_lat: -9.7764,
    },
];

pub fn resolve_region(
    region_code: &str,
    custom_bbox: Option<&str>,
    custom_name: Option<&str>,
) -> Result<RegionSpec, String> {
    if let Some(raw_bbox) = custom_bbox {
        let bbox = BBox::parse(raw_bbox)?;
        let code_upper = region_code.trim().to_ascii_uppercase();
        let slug = code_upper.to_ascii_lowercase().replace('_', "-");
        return Ok(RegionSpec {
            iso_code: code_upper.chars().take(16).collect(),
            ubigeo_code: format!("C-{code_upper}").chars().take(16).collect(),
            name: custom_name
                .map(str::to_string)
                .unwrap_or_else(|| code_upper.clone()),
            slug,
            bbox,
            center_zoom: 8,
            provinces: Vec::new(),
        });
    }

    match region_code.trim().to_ascii_uppercase().as_str() {
        "PE-HUC" | "HUANUCO" => Ok(RegionSpec {
            iso_code: "PE-HUC".to_string(),
            ubigeo_code: "10".to_string(),
            name: "Departamento de Huanuco".to_string(),
            slug: "pe-huc".to_string(),
            bbox: BBox {
                min_lon: -77.35,
                min_lat: -10.55,
                max_lon: -74.55,
                max_lat: -8.35,
            },
            center_zoom: 8,
            provinces: HUANUCO_PROVINCES.to_vec(),
        }),
        "PE-PAS" | "PASCO" => Ok(RegionSpec {
            iso_code: "PE-PAS".to_string(),
            ubigeo_code: "19".to_string(),
            name: "Departamento de Pasco".to_string(),
            slug: "pe-pas".to_string(),
            bbox: BBox {
                min_lon: -76.70,
                min_lat: -11.15,
                max_lon: -74.20,
                max_lat: -9.40,
            },
            center_zoom: 8,
            provinces: PASCO_PROVINCES.to_vec(),
        }),
        "PE-JUN" | "JUNIN" => Ok(RegionSpec {
            iso_code: "PE-JUN".to_string(),
            ubigeo_code: "12".to_string(),
            name: "Departamento de Junin".to_string(),
            slug: "pe-jun".to_string(),
            bbox: BBox {
                min_lon: -76.50,
                min_lat: -12.68,
                max_lon: -73.40,
                max_lat: -10.65,
            },
            center_zoom: 8,
            provinces: JUNIN_PROVINCES.to_vec(),
        }),
        "PE-UCA" | "UCAYALI" => Ok(RegionSpec {
            iso_code: "PE-UCA".to_string(),
            ubigeo_code: "25".to_string(),
            name: "Departamento de Ucayali".to_string(),
            slug: "pe-uca".to_string(),
            bbox: BBox {
                min_lon: -75.95,
                min_lat: -11.45,
                max_lon: -70.45,
                max_lat: -7.30,
            },
            center_zoom: 7,
            provinces: UCAYALI_PROVINCES.to_vec(),
        }),
        "PE-SAM" | "SAN-MARTIN" => Ok(RegionSpec {
            iso_code: "PE-SAM".to_string(),
            ubigeo_code: "22".to_string(),
            name: "Departamento de San Martin".to_string(),
            slug: "pe-sam".to_string(),
            bbox: BBox {
                min_lon: -77.80,
                min_lat: -8.80,
                max_lon: -75.50,
                max_lat: -5.35,
            },
            center_zoom: 7,
            provinces: Vec::new(),
        }),
        "PE-ANC" | "ANCASH" => Ok(RegionSpec {
            iso_code: "PE-ANC".to_string(),
            ubigeo_code: "02".to_string(),
            name: "Departamento de Ancash".to_string(),
            slug: "pe-anc".to_string(),
            bbox: BBox {
                min_lon: -78.65,
                min_lat: -10.80,
                max_lon: -76.70,
                max_lat: -8.00,
            },
            center_zoom: 8,
            provinces: Vec::new(),
        }),
        "PE-ALL" | "PERU" => Ok(RegionSpec {
            iso_code: "PE-ALL".to_string(),
            ubigeo_code: "PE".to_string(),
            name: "Peru Nacional".to_string(),
            slug: "pe-all".to_string(),
            bbox: BBox {
                min_lon: -81.40,
                min_lat: -18.40,
                max_lon: -68.60,
                max_lat: -0.02,
            },
            center_zoom: 6,
            provinces: Vec::new(),
        }),
        other => Err(format!(
            "Unknown region '{other}'. Use PE-HUC, PE-PAS, PE-JUN, PE-UCA, PE-SAM, PE-ANC, PE-ALL or provide --bbox"
        )),
    }
}

/// Seeds the top-level Department (`admin_level=4`) and its official Provinces (`admin_level=6`)
/// into `territorial_units`, returning `(region_unit_id, Vec<(province_unit_id, ProvinceSpec)>)`.
pub async fn seed_region_hierarchy(
    pool: &PgPool,
    region: &RegionSpec,
) -> Result<(Uuid, Vec<(Uuid, ProvinceSpec)>), sqlx::Error> {
    let (cx, cy) = region.bbox.center();
    let region_row = sqlx::query(
        "INSERT INTO territorial_units (
            ubigeo_code, iso_code, parent_unit_id, name, admin_level, bbox_geom, centroid_geom, is_enabled
         ) VALUES (
            $1, $2, NULL, $3, 4,
            ST_GeomFromText($4, 4326),
            ST_SetSRID(ST_MakePoint($5, $6), 4326),
            TRUE
         )
         ON CONFLICT (ubigeo_code) DO UPDATE SET
            iso_code = EXCLUDED.iso_code,
            name = EXCLUDED.name,
            bbox_geom = EXCLUDED.bbox_geom,
            centroid_geom = EXCLUDED.centroid_geom,
            updated_at = NOW()
         RETURNING unit_id",
    )
    .bind(&region.ubigeo_code)
    .bind(&region.iso_code)
    .bind(&region.name)
    .bind(region.bbox.polygon_wkt())
    .bind(cx)
    .bind(cy)
    .fetch_one(pool)
    .await?;

    let region_unit_id: Uuid = region_row.try_get("unit_id")?;
    let mut provinces = Vec::with_capacity(region.provinces.len());

    for prov in &region.provinces {
        let prow = sqlx::query(
            "INSERT INTO territorial_units (
                ubigeo_code, iso_code, parent_unit_id, name, admin_level, bbox_geom, centroid_geom, is_enabled
             ) VALUES (
                $1, $2, $3, $4, 6,
                ST_GeomFromText($5, 4326),
                ST_SetSRID(ST_MakePoint($6, $7), 4326),
                TRUE
             )
             ON CONFLICT (ubigeo_code) DO UPDATE SET
                parent_unit_id = EXCLUDED.parent_unit_id,
                name = EXCLUDED.name,
                bbox_geom = EXCLUDED.bbox_geom,
                centroid_geom = EXCLUDED.centroid_geom,
                updated_at = NOW()
             RETURNING unit_id",
        )
        .bind(prov.ubigeo_code)
        .bind(
            format!("{}-{}", region.iso_code, prov.ubigeo_code)
                .chars()
                .take(16)
                .collect::<String>(),
        )
        .bind(region_unit_id)
        .bind(prov.name)
        .bind(prov.bbox.polygon_wkt())
        .bind(prov.capital_lon)
        .bind(prov.capital_lat)
        .fetch_one(pool)
        .await?;

        provinces.push((prow.try_get("unit_id")?, prov.clone()));
    }

    Ok((region_unit_id, provinces))
}
