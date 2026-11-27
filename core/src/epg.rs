//! Programme guides: Xtream's (`get_simple_data_table`: base64 titles, archive flags) and XMLTV (for M3U).

use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Serialize, Clone)]
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

thread_local! {
    static GUIDE: RefCell<HashMap<String, Vec<Programme>>> = RefCell::new(HashMap::new());
}

// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// XMLTV time, "YYYYMMDDhhmmss +hhmm" (zone optional, UTC if absent), as unix seconds.
fn xmltv_time(s: &str) -> Option<i64> {
    let (t, zone) = s.trim().split_once(' ').unwrap_or((s.trim(), "+0000"));
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let local = days(n(0, 4)?, n(4, 6)?, n(6, 8)?) * 86400 + n(8, 10)? * 3600 + n(10, 12)? * 60 + n(12, 14).unwrap_or(0);
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let z = zone.trim_start_matches(['+', '-']);
    let offset = z.get(0..2)?.parse::<i64>().ok()? * 3600 + z.get(2..4).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0) * 60;
    Some(local - sign * offset)
}

/// Loads an XMLTV guide, replacing any before; returns how many programmes it holds.
pub fn load_xmltv(xml: &str) -> Result<usize, String> {
    let mut reader = Reader::from_str(xml); // untrimmed: an entity splits its text, and trimming would eat the spaces
    let mut guide: HashMap<String, Vec<Programme>> = HashMap::new();
    // The programme being read, and whether its title [0] and description [1] are complete (guides repeat them
    // per language; the first wins).
    let mut current: Option<(String, Programme, [bool; 2])> = None;
    let mut field: Option<usize> = None; // inside <title> (0) or <desc> (1)
    let mut count = 0;
    loop {
        let event = reader.read_event().map_err(|e| format!("guide: {e}"))?;
        match event {
            Event::Start(e) if e.name().as_ref() == "programme" => {
                let get = |k: &str| e.try_get_attribute(k).ok().flatten().and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok()).map(|v| v.into_owned());
                let (Some(channel), Some(start)) = (get("channel"), get("start").and_then(|s| xmltv_time(&s))) else { continue };
                let end = get("stop").and_then(|s| xmltv_time(&s)).unwrap_or(start);
                current = Some((channel, Programme { start, end, title: String::new(), desc: String::new(), archive: false }, [false; 2]));
            }
            Event::Start(e) if current.is_some() => match e.name().as_ref() {
                "title" => field = Some(0),
                "desc" => field = Some(1),
                _ => {}
            },
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                let (Some((_, p, done)), Some(f)) = (current.as_mut(), field) else { continue };
                if done[f] { continue }
                let text = match event {
                    Event::Text(t) => t.into_inner().into_owned(),
                    Event::CData(t) => t.into_inner().into_owned(),
                    Event::GeneralRef(r) => match r.resolve_char_ref() {
                        Ok(Some(c)) => c.to_string(),
                        _ => match r.into_inner().as_ref() { "amp" => "&", "lt" => "<", "gt" => ">", "quot" => "\"", "apos" => "'", _ => "" }.to_string(),
                    },
                    _ => unreachable!(),
                };
                if f == 0 { p.title.push_str(&text) } else { p.desc.push_str(&text) }
            }
            Event::End(e) => match e.name().as_ref() {
                "programme" => {
                    if let Some((channel, mut p, _)) = current.take() {
                        p.title = p.title.trim().to_string();
                        p.desc = p.desc.trim().to_string();
                        guide.entry(channel).or_default().push(p);
                        count += 1;
                    }
                }
                "title" | "desc" => {
                    if let (Some((_, _, done)), Some(f)) = (current.as_mut(), field.take()) { done[f] = true }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    for list in guide.values_mut() {
        list.sort_by_key(|p| p.start);
    }
    GUIDE.with(|g| *g.borrow_mut() = guide);
    Ok(count)
}

/// A channel's programmes from the loaded XMLTV guide, by its tvg-id (also tried without iptv-org's "@SD"-style
/// suffix), within a day before and two after `now`.
pub fn programmes_for(channel: &str, now: i64) -> String {
    GUIDE.with(|g| {
        let g = g.borrow();
        let list = g.get(channel).or_else(|| g.get(channel.split('@').next().unwrap_or(channel))).map_or(&[][..], |v| v);
        let near: Vec<&Programme> = list.iter().filter(|p| p.end > now - 86400 && p.start < now + 2 * 86400).collect();
        serde_json::to_string(&near).unwrap_or_default()
    })
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

    #[test]
    fn reads_xmltv_times() {
        assert_eq!(xmltv_time("19700101000000 +0000"), Some(0));
        assert_eq!(xmltv_time("19700101010000 +0100"), Some(0));
        assert_eq!(xmltv_time("19700101000000 -0130"), Some(5400));
        assert_eq!(xmltv_time("20261127013000"), Some(1795743000));
        assert_eq!(xmltv_time("garbage"), None);
    }

    #[test]
    fn loads_xmltv() {
        let xml = r#"<?xml version="1.0"?><tv>
            <channel id="news.example"><display-name>News</display-name></channel>
            <programme start="20261127020000 +0000" stop="20261127030000 +0000" channel="news.example">
              <title lang="en">Late &amp; Live</title><title lang="fr">Tard</title><desc>Talk &#233;dition</desc></programme>
            <programme start="20261127010000 +0000" stop="20261127020000 +0000" channel="news.example"><title>Earlier</title></programme>
            <programme start="bad" channel="x"><title>skipped</title></programme>
        </tv>"#;
        assert_eq!(load_xmltv(xml), Ok(2));
        let v: Value = serde_json::from_str(&programmes_for("news.example@HD", 1795744800)).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2, "found without the @HD suffix");
        assert_eq!(v[0]["title"], "Earlier", "sorted by start");
        assert_eq!(v[1]["title"], "Late & Live", "first title only, entities resolved");
        assert_eq!(v[1]["desc"], "Talk édition");
        assert_eq!(programmes_for("nope", 0), "[]");
    }
}
