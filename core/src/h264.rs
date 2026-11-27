//! Which H.264 access units are single fields. Broadcast 1080i is often PAFF: each field is its own picture
//! (and its own MPEG-TS packet), but an MP4 sample must hold a whole frame, so the remuxer pairs them.

/// Reads Exp-Golomb and fixed-width fields from an RBSP (emulation-prevention bytes removed).
struct Bits {
    b: Vec<u8>,
    i: usize,
}

impl Bits {
    fn new(nal: &[u8]) -> Self {
        let mut b = Vec::with_capacity(nal.len());
        let mut zeros = 0;
        for &x in nal {
            if zeros >= 2 && x == 3 {
                zeros = 0;
                continue;
            }
            zeros = if x == 0 { zeros + 1 } else { 0 };
            b.push(x);
        }
        Bits { b, i: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let v = (self.b.get(self.i / 8)? >> (7 - self.i % 8)) & 1;
        self.i += 1;
        Some(v as u32)
    }

    fn u(&mut self, n: u32) -> Option<u32> {
        (0..n).try_fold(0, |v, _| Some((v << 1) | self.bit()?))
    }

    fn ue(&mut self) -> Option<u32> {
        let mut z = 0;
        while self.bit()? == 0 {
            z += 1;
            if z > 31 {
                return None;
            }
        }
        Some((1u32 << z) - 1 + self.u(z)?)
    }

    fn se(&mut self) -> Option<i32> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as i32)
    }
}

/// What a slice header needs from the SPS.
#[derive(Clone, Copy, Default)]
pub struct Sps {
    frame_num_bits: u32,
    frame_mbs_only: bool,
    separate_colour_plane: bool,
}

fn parse_sps(nal: &[u8]) -> Option<Sps> {
    let mut r = Bits::new(&nal[1..]);
    let profile = r.u(8)?;
    r.u(16)?; // constraint flags, level
    r.ue()?; // sps id
    let mut separate_colour_plane = false;
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profile) {
        let chroma = r.ue()?;
        if chroma == 3 {
            separate_colour_plane = r.bit()? == 1;
        }
        r.ue()?; // bit depth luma
        r.ue()?; // bit depth chroma
        r.bit()?; // qpprime_y_zero_transform_bypass
        if r.bit()? == 1 {
            for i in 0..if chroma == 3 { 12 } else { 8 } {
                if r.bit()? == 1 {
                    let size = if i < 6 { 16 } else { 64 };
                    let (mut last, mut next) = (8i32, 8i32);
                    for _ in 0..size {
                        if next != 0 {
                            next = (last + r.se()? + 256) % 256;
                        }
                        last = if next == 0 { last } else { next };
                    }
                }
            }
        }
    }
    let frame_num_bits = r.ue()? + 4;
    match r.ue()? {
        0 => {
            r.ue()?;
        }
        1 => {
            r.bit()?;
            r.se()?;
            r.se()?;
            for _ in 0..r.ue()? {
                r.se()?;
            }
        }
        _ => {}
    }
    r.ue()?; // max ref frames
    r.bit()?; // gaps allowed
    r.ue()?; // width in mbs
    r.ue()?; // height in map units
    Some(Sps { frame_num_bits, frame_mbs_only: r.bit()? == 1, separate_colour_plane })
}

/// NAL units of an Annex B access unit (start codes stripped).
fn nals(au: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut starts = vec![];
    let mut i = 0;
    while i + 2 < au.len() {
        if au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let ends: Vec<usize> = starts.iter().skip(1).map(|&s| s - 3).chain([au.len()]).collect();
    starts.into_iter().zip(ends).map(move |(s, e)| {
        let mut e = e;
        while e > s && au[e - 1] == 0 {
            e -= 1; // trailing zero of a 4-byte start code
        }
        &au[s..e]
    })
}

/// A field picture: its frame_num and whether it's the bottom field.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Field {
    pub frame_num: u32,
    pub bottom: bool,
}

/// Classifies one access unit, updating `sps` from any SPS it carries. `None`: a frame (or not a picture).
pub fn field(sps: &mut Option<Sps>, au: &[u8]) -> Option<Field> {
    for nal in nals(au) {
        match nal.first()? & 31 {
            7 => *sps = parse_sps(nal).or(*sps),
            1 | 5 => {
                let s = (*sps)?;
                if s.frame_mbs_only {
                    return None;
                }
                let mut r = Bits::new(&nal[1..]);
                r.ue()?; // first_mb_in_slice
                r.ue()?; // slice_type
                r.ue()?; // pps id
                if s.separate_colour_plane {
                    r.u(2)?;
                }
                let frame_num = r.u(s.frame_num_bits)?;
                return (r.bit()? == 1).then(|| Some(Field { frame_num, bottom: r.bit()? == 1 })).flatten();
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real bytes, from a 1080i broadcast (Main, PAFF) and an x264 1080p stream; expectations per ffmpeg's trace_headers.
    const SPS_1080I: &[u8] = &[0x67, 0x4d, 0x40, 0x28, 0x9a, 0x52, 0x80, 0xf0, 0x08, 0x8f, 0xbc, 0x05, 0x08, 0x08, 0x08, 0x0a, 0x00, 0x00, 0x03, 0x00, 0x02, 0x00, 0x00, 0x03, 0x00, 0x65, 0x28];
    const TOP_130: &[u8] = &[0x41, 0x88, 0xa0, 0xa0, 0x4a, 0x69, 0x44, 0xca, 0x5f, 0xe9, 0x72, 0x48];
    const BOTTOM_130: &[u8] = &[0x41, 0x9a, 0x82, 0xc1, 0x7e, 0x4c, 0x72, 0x97, 0xa7, 0xac, 0x4d, 0x69];
    const SPS_1080P: &[u8] = &[0x67, 0x4d, 0x40, 0x28, 0xec, 0xa0, 0x3c, 0x01, 0x13, 0xf2, 0xe0, 0x22, 0x00, 0x00, 0x03, 0x00, 0x02, 0x00, 0x00, 0x03, 0x00, 0x64, 0x1e, 0x30, 0x63, 0x2c];
    const IDR_1080P: &[u8] = &[0x65, 0x88, 0x84, 0x03, 0xff, 0xad, 0x6e, 0xda, 0xc4, 0xf1, 0x5a, 0x80];

    fn au(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter().flat_map(|n| [&[0, 0, 0, 1][..], n].concat()).collect()
    }

    #[test]
    fn reads_exp_golomb() {
        let mut r = Bits::new(&[0b1010_0110, 0b1100_0000]); // 1 010 011 011: ue 0, 1, 2; se -1
        assert_eq!((r.ue(), r.ue(), r.ue(), r.se()), (Some(0), Some(1), Some(2), Some(-1)));
    }

    #[test]
    fn strips_emulation_prevention() {
        assert_eq!(Bits::new(&[0, 0, 3, 1, 0, 0, 3]).b, vec![0, 0, 1, 0, 0]);
    }

    #[test]
    fn finds_paff_fields() {
        let mut sps = None;
        assert_eq!(field(&mut sps, &au(&[&[0x09, 0x10], SPS_1080I, TOP_130])), Some(Field { frame_num: 130, bottom: false }));
        assert_eq!(field(&mut sps, &au(&[&[0x09, 0x30], BOTTOM_130])), Some(Field { frame_num: 130, bottom: true }), "SPS remembered");
    }

    #[test]
    fn progressive_is_frames() {
        let mut sps = None;
        assert_eq!(field(&mut sps, &au(&[SPS_1080P, IDR_1080P])), None);
        assert_eq!(field(&mut None, &au(&[TOP_130])), None, "no SPS seen yet");
    }
}

thread_local! {
    static SPS: std::cell::Cell<Option<Sps>> = const { std::cell::Cell::new(None) };
}

/// Forget the SPS (a new stream).
#[no_mangle]
pub extern "C" fn h264_reset() {
    SPS.set(None);
}

/// -1 for a frame, else `frame_num << 1 | bottom` for a field picture.
///
/// # Safety
/// `p`/`n` must point at `n` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn h264_field(p: *const u8, n: usize) -> i32 {
    let mut sps = SPS.get();
    let f = field(&mut sps, std::slice::from_raw_parts(p, n));
    SPS.set(sps);
    f.map_or(-1, |f| ((f.frame_num as i32) << 1) | f.bottom as i32)
}

/// Whether the last SPS seen allows interlaced coding (fields or MBAFF), so the player knows to deinterlace.
#[no_mangle]
pub extern "C" fn h264_interlaced() -> bool {
    SPS.get().is_some_and(|s| !s.frame_mbs_only)
}
