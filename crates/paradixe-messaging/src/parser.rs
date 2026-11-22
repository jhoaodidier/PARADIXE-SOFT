#![forbid(unsafe_code)]

use paradixe_domain::ProposedPassability;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "intent", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CitizenIntent {
    ReportRoad {
        passability: ProposedPassability,
        corridor_query: String,
    },
    QueryCorridor {
        corridor_query: String,
    },
    Help,
}

fn normalize_spanish_ascii(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let mapped = match ch {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            other => other.to_ascii_lowercase(),
        };
        out.push(mapped);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_leading_prepositions(text: &str) -> String {
    let trimmed = text
        .trim_matches(|c: char| c == '?' || c == '¿' || c == '!' || c == '¡' || c.is_whitespace());
    let mut words: Vec<&str> = trimmed.split_whitespace().collect();
    while let Some(&first) = words.first() {
        if matches!(
            first,
            "en" | "de" | "del" | "la" | "el" | "por" | "hacia" | "sobre" | "sector" | "tramo"
        ) {
            words.remove(0);
        } else {
            break;
        }
    }
    words
        .join(" ")
        .trim_matches(|c: char| c == '?' || c == '¿' || c == '!' || c == '¡' || c == '.')
        .trim()
        .to_string()
}

fn extract_corridor_after_keywords(normalized: &str, keywords: &[&str]) -> String {
    for kw in keywords {
        if let Some(idx) = normalized.find(kw) {
            let after = normalized[idx + kw.len()..].trim();
            let cleaned = strip_leading_prepositions(after);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }
    strip_leading_prepositions(normalized)
}

/// Parses free-text WhatsApp/SMS messages in Andean Spanish into deterministic commands.
pub fn parse_citizen_message(raw: &str) -> CitizenIntent {
    let norm = normalize_spanish_ascii(raw);
    if norm.is_empty() || matches!(norm.as_str(), "hola" | "ayuda" | "menu" | "?" | "info") {
        return CitizenIntent::Help;
    }

    let blocked_kw = [
        "bloqueado",
        "bloqueo",
        "derrumbe",
        "huaico",
        "huayco",
        "cerrado",
        "cerrada",
        "cortado",
        "corte",
        "colapso",
    ];
    if blocked_kw.iter().any(|k| norm.contains(k)) {
        return CitizenIntent::ReportRoad {
            passability: ProposedPassability::Blocked,
            corridor_query: extract_corridor_after_keywords(&norm, &blocked_kw),
        };
    }

    let partial_kw = [
        "1 carril",
        "un carril",
        "paso restringido",
        "restringido",
        "parcial",
        "solo un carril",
    ];
    if partial_kw.iter().any(|k| norm.contains(k)) {
        return CitizenIntent::ReportRoad {
            passability: ProposedPassability::PartialOneLane,
            corridor_query: extract_corridor_after_keywords(&norm, &partial_kw),
        };
    }

    let open_kw = ["despejado", "despejada", "habilitado", "abierto", "libre"];
    if open_kw.iter().any(|k| norm.contains(k)) {
        return CitizenIntent::ReportRoad {
            passability: ProposedPassability::Open,
            corridor_query: extract_corridor_after_keywords(&norm, &open_kw),
        };
    }

    let query_kw = [
        "como esta la via",
        "como esta la carretera",
        "estado de la via",
        "como esta",
        "estado de",
        "estado",
        "clima",
        "ruta",
        "via",
        "carretera",
    ];
    if query_kw.iter().any(|k| norm.contains(k)) {
        return CitizenIntent::QueryCorridor {
            corridor_query: extract_corridor_after_keywords(&norm, &query_kw),
        };
    }

    CitizenIntent::Help
}

/// Ensures SMS responses fit comfortably inside a single GSM/UTF-8 160-character payload.
pub fn truncate_sms_160(msg: &str) -> String {
    let count = msg.chars().count();
    if count <= 160 {
        return msg.to_string();
    }
    let truncated: String = msg.chars().take(157).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spanish_accents_and_road_reports() {
        assert_eq!(
            parse_citizen_message("DERRUMBE en Túnel Carpish"),
            CitizenIntent::ReportRoad {
                passability: ProposedPassability::Blocked,
                corridor_query: "tunel carpish".to_string(),
            }
        );
        assert_eq!(
            parse_citizen_message("Paso restringido 1 carril Carretera Central"),
            CitizenIntent::ReportRoad {
                passability: ProposedPassability::PartialOneLane,
                corridor_query: "carretera central".to_string(),
            }
        );
        assert_eq!(
            parse_citizen_message("Ya está despejado en Ambo"),
            CitizenIntent::ReportRoad {
                passability: ProposedPassability::Open,
                corridor_query: "ambo".to_string(),
            }
        );
        assert_eq!(
            parse_citizen_message("¿Cómo está la vía Tingo María?"),
            CitizenIntent::QueryCorridor {
                corridor_query: "tingo maria".to_string(),
            }
        );
        assert_eq!(parse_citizen_message("ayuda"), CitizenIntent::Help);
    }

    #[test]
    fn sms_truncation_preserves_char_limit() {
        let long = "PARADIXE: ".repeat(30);
        let sms = truncate_sms_160(&long);
        assert!(sms.chars().count() <= 160);
        assert!(sms.ends_with("..."));
    }
}
