//! Xtream stream lists (live, vod, series): held here once loaded, filtered and searched without JS ever
//! holding the multi-MB arrays.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Serialize, Clone)]
pub struct Item {
    pub id: u64,
    pub name: String,
    pub icon: String,
    pub cat: String,
    /// vod: container extension; live: epg channel id.
    pub ext: String,
    /// live: catch-up days (0 = none).
    pub archive: u32,
    pub rating: f32,
    #[serde(skip)]
    key: String,
}

thread_local! {
    static LISTS: RefCell<HashMap<String, Vec<Item>>> = RefCell::new(HashMap::new());
}

/// Lower-case, compatibility-folded, accent-free: "News ᶠᴴᴰ" → "news fhd", "Ação" → "acao".
pub fn fold(s: &str) -> String {
    s.nfkd().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).flat_map(char::to_lowercase).collect()
}

/// Xtream servers send numbers as numbers or strings, depending on the field and the server.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

#[derive(Deserialize)]
struct Raw(Vec<HashMap<String, Value>>);

pub fn load(kind: &str, json: &str) -> Result<usize, String> {
    let Raw(rows) = serde_json::from_str(json).map_err(|e| format!("{kind} list: {e}"))?;
    let null = Value::Null;
    let items: Vec<Item> = rows
        .iter()
        .map(|r| {
            let g = |k: &str| r.get(k).unwrap_or(&null);
            let name = text(g("name"));
            Item {
                id: num(if kind == "series" { g("series_id") } else { g("stream_id") }) as u64,
                key: fold(&name),
                icon: text(if kind == "series" { g("cover") } else { g("stream_icon") }),
                cat: text(g("category_id")),
                ext: text(if kind == "live" { g("epg_channel_id") } else { g("container_extension") }),
                archive: if num(g("tv_archive")) > 0.0 { num(g("tv_archive_duration")) as u32 } else { 0 },
                rating: num(g("rating")) as f32,
                name,
            }
        })
        .collect();
    let n = items.len();
    LISTS.with(|l| l.borrow_mut().insert(kind.to_string(), items));
    Ok(n)
}

pub fn query(kind: &str, cat: &str, offset: usize, limit: usize, ids: Option<&[u64]>, q: &str) -> Result<String, String> {
    let words: Vec<String> = fold(q).split_whitespace().map(str::to_string).collect();
    LISTS.with(|l| {
        let l = l.borrow();
        let all = l.get(kind).ok_or_else(|| format!("{kind} not loaded"))?;
        let hits: Vec<&Item> = all
            .iter()
            .filter(|i| cat.is_empty() || i.cat == cat)
            .filter(|i| ids.is_none_or(|ids| ids.contains(&i.id)))
            .filter(|i| words.iter().all(|w| i.key.contains(w.as_str())))
            .collect();
        let page: Vec<&Item> = hits.iter().skip(offset).take(limit).copied().collect();
        Ok(serde_json::json!({ "total": hits.len(), "items": page }).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_superscripts_and_accents() {
        assert_eq!(fold("News ᶠᴴᴰ"), "news fhd");
        assert_eq!(fold("Ação Ñ"), "acao n");
    }

    #[test]
    fn loads_and_searches() {
        let json = r#"[
            {"name":"News ᶠᴴᴰ","stream_id":2,"stream_icon":"i","category_id":"1","epg_channel_id":"news.example","tv_archive":1,"tv_archive_duration":"3"},
            {"name":"Café Música","stream_id":"5","category_id":2,"tv_archive":0}
        ]"#;
        assert_eq!(load("live", json), Ok(2));
        let r: Value = serde_json::from_str(&query("live", "", 0, 10, None, "musica").unwrap()).unwrap();
        assert_eq!(r["total"], 1);
        assert_eq!(r["items"][0]["id"], 5);
        assert_eq!(r["items"][0]["cat"], "2");
        let r: Value = serde_json::from_str(&query("live", "1", 0, 10, None, "news fhd").unwrap()).unwrap();
        assert_eq!(r["items"][0]["archive"], 3);
        let r: Value = serde_json::from_str(&query("live", "", 0, 10, Some(&[5]), "").unwrap()).unwrap();
        assert_eq!(r["total"], 1);
    }
}
