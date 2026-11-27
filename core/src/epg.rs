//! Xtream EPG (`get_simple_data_table`): base64 titles and archive flags.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct Programme {
    start: i64,
    end: i64,
    title: String,
    desc: String,
    /// Catch-up available: the server recorded it and it has finished.
    archive: bool,
}

fn b64(v: &Value) -> String {
    let s = v.as_str().unwrap_or_default();
    STANDARD.decode(s).ok().and_then(|b| String::from_utf8(b).ok()).unwrap_or_else(|| s.to_string())
}

fn int(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

pub fn programmes(json: &str, now: i64) -> Result<String, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("epg: {e}"))?;
    let list: Vec<Programme> = v["epg_listings"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|p| {
            let end = int(&p["stop_timestamp"]);
            Programme {
                start: int(&p["start_timestamp"]),
                end,
                title: b64(&p["title"]),
                desc: b64(&p["description"]),
                archive: int(&p["has_archive"]) == 1 && end <= now,
            }
        })
        .collect();
    serde_json::to_string(&list).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_listing() {
        let json = r#"{"epg_listings":[{"title":"R3JhbmRlIEVudHJldmlzdGE=","description":"","start":"2026-11-24 00:18:18",
            "start_timestamp":"100","stop_timestamp":"200","has_archive":1}]}"#;
        let v: Value = serde_json::from_str(&programmes(json, 300).unwrap()).unwrap();
        assert_eq!(v[0]["title"], "Grande Entrevista");
        assert_eq!(v[0]["archive"], true);
        let v: Value = serde_json::from_str(&programmes(json, 150).unwrap()).unwrap();
        assert_eq!(v[0]["archive"], false, "still airing");
    }
}
