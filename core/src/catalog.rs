//! Stream lists (Xtream live/vod/series, or an M3U playlist): held here once loaded, filtered and searched
//! without JS ever holding the multi-MB arrays.

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
    /// M3U: the stream itself (Xtream builds it from the id).
    pub url: String,
    /// M3U: country code from the tvg-id ("BBCOne.uk@HD" → "uk"), or "".
    pub country: String,
    /// M3U: what the playlist says about playing it (free checks, no request): "referrer" (needs a Referer a
    /// page can't send), "agent" (needs a non-browser User-Agent), "insecure" (http://), "geo" (region-locked),
    /// "part-time" (not on air 24/7).
    pub notes: Vec<&'static str>,
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
                url: String::new(),
                country: String::new(),
                notes: Vec::new(),
                name,
            }
        })
        .collect();
    let n = items.len();
    LISTS.with(|l| l.borrow_mut().insert(kind.to_string(), items));
    Ok(n)
}

fn in_cat(i: &Item, cat: &str) -> bool {
    cat.is_empty() || i.cat.split(';').any(|c| c.trim() == cat)
}

fn in_country(i: &Item, country: &str) -> bool {
    country.is_empty() || i.country == country
}

/// `exclude`: comma-separated notes; an item with any of them is left out.
fn allowed(i: &Item, exclude: &str) -> bool {
    exclude.is_empty() || !i.notes.iter().any(|n| exclude.split(',').any(|x| x == *n))
}

#[allow(clippy::too_many_arguments)]
pub fn query(kind: &str, cat: &str, country: &str, exclude: &str, offset: usize, limit: usize, ids: Option<&[u64]>, q: &str) -> Result<String, String> {
    let words: Vec<String> = fold(q).split_whitespace().map(str::to_string).collect();
    LISTS.with(|l| {
        let l = l.borrow();
        let all = l.get(kind).ok_or_else(|| format!("{kind} not loaded"))?;
        let hits: Vec<&Item> = all
            .iter()
            .filter(|i| in_cat(i, cat) && in_country(i, country) && allowed(i, exclude))
            .filter(|i| ids.is_none_or(|ids| ids.contains(&i.id)))
            .filter(|i| words.iter().all(|w| i.key.contains(w.as_str())))
            .collect();
        let page: Vec<&Item> = hits.iter().skip(offset).take(limit).copied().collect();
        Ok(serde_json::json!({ "total": hits.len(), "items": page }).to_string())
    })
}

/// The value of `key="…"` in an #EXTINF line ("" if absent).
fn attr(line: &str, key: &str) -> String {
    let pat = format!("{key}=\"");
    line.find(&pat)
        .and_then(|i| line[i + pat.len()..].split('"').next())
        .unwrap_or_default()
        .to_string()
}

/// Loads an M3U playlist as the live list: each #EXTINF line names the stream on the next URL line, with any
/// #EXTVLCOPT player options in between.
pub fn load_m3u(text: &str) -> usize {
    let mut items = Vec::new();
    let mut info: Option<&str> = None;
    let mut opts: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            info = Some(rest);
            opts.clear();
        } else if let Some(opt) = line.strip_prefix("#EXTVLCOPT:") {
            opts.push(opt);
        } else if !line.is_empty() && !line.starts_with('#') {
            let Some(rest) = info.take() else { continue };
            // The title follows the first comma outside the quoted attributes.
            let mut quoted = false;
            let comma = rest.char_indices().find(|&(_, c)| {
                quoted ^= c == '"';
                c == ',' && !quoted
            });
            let title = comma.map_or("", |(i, _)| &rest[i + 1..]).trim();
            let opt = |k: &str| opts.iter().find_map(|o| o.strip_prefix(k)?.strip_prefix('=')).unwrap_or_default().to_string();
            let agent = Some(attr(rest, "http-user-agent")).filter(|a| !a.is_empty()).unwrap_or_else(|| opt("http-user-agent"));
            let referrer = attr(rest, "http-referrer") + &opt("http-referrer");
            let mut notes = Vec::new();
            if !referrer.is_empty() { notes.push("referrer") }
            // A browser-like User-Agent is what the browser sends anyway; anything else it can't.
            if !agent.is_empty() && !agent.starts_with("Mozilla/") { notes.push("agent") }
            if line.starts_with("http://") { notes.push("insecure") }
            if title.contains("[Geo-blocked]") { notes.push("geo") }
            if title.contains("[Not 24/7]") { notes.push("part-time") }
            let name = title.replace(" [Geo-blocked]", "").replace(" [Not 24/7]", "").replace("[Geo-blocked]", "").replace("[Not 24/7]", "").trim().to_string();
            let tvg = attr(rest, "tvg-id");
            let cc = tvg.split('@').next().and_then(|id| id.rsplit_once('.')).map_or("", |(_, cc)| cc).to_ascii_lowercase();
            items.push(Item {
                id: items.len() as u64,
                key: fold(&name),
                icon: attr(rest, "tvg-logo"),
                cat: attr(rest, "group-title"),
                country: if cc.len() == 2 && cc.bytes().all(|b| b.is_ascii_alphabetic()) { cc } else { String::new() },
                ext: tvg,
                archive: 0,
                rating: 0.0,
                url: line.to_string(),
                notes,
                name,
            });
        }
    }
    let n = items.len();
    LISTS.with(|l| l.borrow_mut().insert("live".to_string(), items));
    n
}

/// Counts of `key(item)` values over a list's items that pass `keep`, as [{id, count}] by count.
fn tally(kind: &str, keep: impl Fn(&Item) -> bool, keys: impl Fn(&Item) -> Vec<String>) -> String {
    LISTS.with(|l| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for i in l.borrow().get(kind).map_or(&[][..], |v| v).iter().filter(|i| keep(i)) {
            for k in keys(i) {
                *counts.entry(k).or_default() += 1;
            }
        }
        let mut list: Vec<(String, usize)> = counts.into_iter().collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let json: Vec<Value> = list.into_iter().map(|(id, count)| serde_json::json!({ "id": id, "count": count })).collect();
        serde_json::to_string(&json).unwrap_or_default()
    })
}

/// A list's groups (M3U categories) with channel counts, within `country` and without `exclude`d notes.
pub fn categories(kind: &str, country: &str, exclude: &str) -> String {
    tally(kind, |i| in_country(i, country) && allowed(i, exclude), |i| {
        i.cat.split(';').map(str::trim).filter(|g| !g.is_empty()).map(str::to_string).collect()
    })
}

/// A list's countries with channel counts, within `cat` and without `exclude`d notes.
pub fn countries(kind: &str, cat: &str, exclude: &str) -> String {
    tally(kind, |i| in_cat(i, cat) && allowed(i, exclude) && !i.country.is_empty(), |i| vec![i.country.clone()])
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
        let r: Value = serde_json::from_str(&query("live", "", "", "", 0, 10, None, "musica").unwrap()).unwrap();
        assert_eq!(r["total"], 1);
        assert_eq!(r["items"][0]["id"], 5);
        assert_eq!(r["items"][0]["cat"], "2");
        let r: Value = serde_json::from_str(&query("live", "1", "", "", 0, 10, None, "news fhd").unwrap()).unwrap();
        assert_eq!(r["items"][0]["archive"], 3);
        let r: Value = serde_json::from_str(&query("live", "", "", "", 0, 10, Some(&[5]), "").unwrap()).unwrap();
        assert_eq!(r["total"], 1);
    }

    #[test]
    fn loads_m3u() {
        let m3u = "#EXTM3U\n#EXTINF:-1 tvg-id=\"news.example@HD\" tvg-logo=\"https://l/n.png\" group-title=\"News;General\",News, Live (1080p)\n\
                   #EXTVLCOPT:http-user-agent=x\nhttps://s/news.m3u8\n#EXTINF:-1 tvg-id=\"Music.FR@SD\" group-title=\"Music;General\",Café Música\nhttps://s/m.m3u8\n";
        assert_eq!(load_m3u(m3u), 2);
        let r: Value = serde_json::from_str(&query("live", "General", "", "", 0, 10, None, "").unwrap()).unwrap();
        assert_eq!(r["items"][0]["name"], "News, Live (1080p)", "comma in the title");
        assert_eq!(r["items"][0]["url"], "https://s/news.m3u8");
        assert_eq!(r["items"][0]["ext"], "news.example@HD");
        assert_eq!(r["items"][0]["country"], "", "\"example\" isn't a country code");
        assert_eq!(r["items"][1]["country"], "fr", "two letters, lower-cased");
        let facet = |json: String| serde_json::from_str::<Value>(&json).unwrap();
        assert_eq!(facet(categories("live", "", "")), serde_json::json!([{ "id": "General", "count": 2 }, { "id": "Music", "count": 1 }, { "id": "News", "count": 1 }]));
        assert_eq!(facet(categories("live", "fr", "")), serde_json::json!([{ "id": "General", "count": 1 }, { "id": "Music", "count": 1 }]));
        assert_eq!(facet(countries("live", "News", "")), serde_json::json!([]));
        assert_eq!(facet(countries("live", "", "")), serde_json::json!([{ "id": "fr", "count": 1 }]));
        let r: Value = serde_json::from_str(&query("live", "General", "fr", "", 0, 10, None, "").unwrap()).unwrap();
        assert_eq!(r["total"], 1, "category and country combine");
    }

    #[test]
    fn notes_what_the_playlist_says() {
        let m3u = "#EXTM3U\n\
            #EXTINF:-1 http-user-agent=\"Mozilla/5.0 (Windows NT 10.0)\",Browser Agent\nhttps://s/a.m3u8\n\
            #EXTINF:-1,Box Agent [Geo-blocked]\n#EXTVLCOPT:http-user-agent=otg/1.5.1 (AppleTv)\nhttp://s/b.m3u8\n\
            #EXTINF:-1 http-referrer=\"https://site.example/\",Needs Referrer [Not 24/7]\nhttps://s/c.m3u8\n";
        load_m3u(m3u);
        let r: Value = serde_json::from_str(&query("live", "", "", "", 0, 10, None, "").unwrap()).unwrap();
        let notes = |i: usize| r["items"][i]["notes"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect::<Vec<_>>();
        assert!(notes(0).is_empty(), "a browser-like agent is what the browser sends");
        assert_eq!(notes(1), ["agent", "insecure", "geo"]);
        assert_eq!(r["items"][1]["name"], "Box Agent", "tags move from the name to notes");
        assert_eq!(notes(2), ["referrer", "part-time"]);
        let r: Value = serde_json::from_str(&query("live", "", "", "referrer,agent,geo", 0, 10, None, "").unwrap()).unwrap();
        assert_eq!(r["total"], 1, "excluded notes leave just the browser-agent channel");
    }
}
