//! iptvc's wasm core (no wasm-bindgen; same ABI shape as puzzles/crates/wasm).
//!
//! Text ops: JS writes a UTF-8 request into `ic_alloc(len)`, calls `ic_call(op, ptr, len)` (fields separated by
//! `\n`, except the last which may hold anything), then reads the reply at `ic_out_ptr()`/`ic_out_len()`;
//! a non-zero return means the reply is an error message.
//! Audio: `mpa_*` decode MPEG-1/2 audio frames (MP2 is what DVB channels carry, and browsers can't decode it).

pub mod catalog;
pub mod epg;
pub mod h264;
pub mod mpa;

use std::cell::RefCell;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_out(bytes: Vec<u8>) {
    OUT.with(|o| *o.borrow_mut() = bytes);
}

#[no_mangle]
pub extern "C" fn ic_alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p`/`n` must come from `ic_alloc(n)`.
#[no_mangle]
pub unsafe extern "C" fn ic_free(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, 0, n));
}

#[no_mangle]
pub extern "C" fn ic_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn ic_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// # Safety
/// `p`/`n` must point at `n` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn ic_call(op: u32, p: *const u8, n: usize) -> i32 {
    let req = String::from_utf8_lossy(std::slice::from_raw_parts(p, n));
    match dispatch(op, &req) {
        Ok(s) => {
            set_out(s.into_bytes());
            0
        }
        Err(e) => {
            set_out(e.into_bytes());
            1
        }
    }
}

fn fields(req: &str, n: usize) -> Vec<&str> {
    let mut v: Vec<&str> = req.splitn(n, '\n').collect();
    v.resize(n, "");
    v
}

fn dispatch(op: u32, req: &str) -> Result<String, String> {
    match op {
        // load(kind, json list) -> count
        0 => {
            let f = fields(req, 2);
            catalog::load(f[0], f[1]).map(|n| n.to_string())
        }
        // query(kind, category ("" = all), country ("" = all), excluded notes (comma list), offset, limit,
        //   ids ("" = any, else comma list), text) -> {total, items}
        1 => {
            let f = fields(req, 8);
            let num = |s: &str| s.parse::<usize>().map_err(|e| e.to_string());
            let ids: Vec<u64> = f[6].split(',').filter_map(|s| s.parse().ok()).collect();
            catalog::query(f[0], f[1], f[2], f[3], num(f[4])?, num(f[5])?, (!f[6].is_empty()).then_some(&ids[..]), f[7])
        }
        // epg(now unix seconds, get_simple_data_table json) -> programmes
        2 => {
            let f = fields(req, 2);
            epg::programmes(f[1], f[0].parse().map_err(|_| "bad now")?)
        }
        // load_m3u(playlist) -> count (the live list)
        4 => Ok(catalog::load_m3u(req).to_string()),
        // categories(kind, country, excluded notes) -> [{id, count}] from the list's groups
        5 => {
            let f = fields(req, 3);
            Ok(catalog::categories(f[0], f[1], f[2]))
        }
        // load_xmltv(guide) -> programme count
        6 => epg::load_xmltv(req).map(|n| n.to_string()),
        // guide(now unix seconds, channel tvg-id) -> programmes, from the XMLTV guide
        7 => {
            let f = fields(req, 2);
            Ok(epg::programmes_for(f[1], f[0].parse().map_err(|_| "bad now")?))
        }
        // countries(kind, category, excluded notes) -> [{id, count}]
        8 => {
            let f = fields(req, 3);
            Ok(catalog::countries(f[0], f[1], f[2]))
        }
        _ => Err(format!("unknown op {op}")),
    }
}
