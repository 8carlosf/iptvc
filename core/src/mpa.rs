//! MPEG-1/2 audio layers I–III (symphonia), one frame per call. Browsers decode MP3 but not MP2, and DVB
//! channels (so most live IPTV) carry MP2.

use crate::set_out;
use std::cell::RefCell;
use symphonia_bundle_mp3::MpaDecoder;
use symphonia_core::codecs::audio::well_known::{CODEC_ID_MP1, CODEC_ID_MP2, CODEC_ID_MP3};
use symphonia_core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia_core::packet::PacketRef;
use symphonia_core::units::{Duration, Timestamp};

#[derive(Default)]
struct Slot {
    dec: Option<(u8, MpaDecoder)>,
    rate: u32,
    channels: u32,
}

thread_local! {
    static SLOTS: RefCell<Vec<Option<Slot>>> = const { RefCell::new(Vec::new()) };
}

/// Decodes one frame into interleaved f32 samples; `None` for a frame the decoder rejects (skipped, as a
/// player should on a corrupt frame).
fn decode(slot: &mut Slot, frame: &[u8]) -> Option<Vec<f32>> {
    let layer = (*frame.get(1)? >> 1) & 3; // 3 = layer I, 2 = II, 1 = III
    if slot.dec.as_ref().is_none_or(|(l, _)| *l != layer) {
        let codec = [None, Some(CODEC_ID_MP3), Some(CODEC_ID_MP2), Some(CODEC_ID_MP1)][layer as usize]?;
        let dec = MpaDecoder::try_new(AudioCodecParameters::new().for_codec(codec), &AudioDecoderOptions::default()).ok()?;
        slot.dec = Some((layer, dec));
    }
    let (_, dec) = slot.dec.as_mut()?;
    let buf = dec.decode_ref(&PacketRef::new(0, Timestamp::new(0), Duration::new(0), frame)).ok()?;
    slot.rate = buf.spec().rate();
    slot.channels = buf.spec().channels().count() as u32;
    let mut out = Vec::new();
    buf.copy_to_vec_interleaved(&mut out);
    Some(out)
}

#[no_mangle]
pub extern "C" fn mpa_open() -> u32 {
    SLOTS.with(|s| {
        let mut s = s.borrow_mut();
        let i = s.iter().position(Option::is_none).unwrap_or_else(|| {
            s.push(None);
            s.len() - 1
        });
        s[i] = Some(Slot::default());
        i as u32
    })
}

#[no_mangle]
pub extern "C" fn mpa_close(h: u32) {
    SLOTS.with(|s| s.borrow_mut().get_mut(h as usize).map(Option::take));
}

/// Decodes the frame at `p`/`n`; the samples (interleaved f32) are at `ic_out_ptr()`. Returns frames per
/// channel, 0 if the frame was skipped. `mpa_rate`/`mpa_channels` describe the last decoded frame.
///
/// # Safety
/// `p`/`n` must point at `n` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn mpa_decode(h: u32, p: *const u8, n: usize) -> u32 {
    let frame = std::slice::from_raw_parts(p, n);
    SLOTS.with(|s| {
        let mut s = s.borrow_mut();
        let Some(Some(slot)) = s.get_mut(h as usize) else { return 0 };
        let pcm = decode(slot, frame).unwrap_or_default();
        let frames = pcm.len() as u32 / slot.channels.max(1);
        set_out(pcm.iter().flat_map(|x| x.to_le_bytes()).collect());
        frames
    })
}

#[no_mangle]
pub extern "C" fn mpa_rate(h: u32) -> u32 {
    SLOTS.with(|s| s.borrow().get(h as usize).and_then(Option::as_ref).map_or(0, |x| x.rate))
}

#[no_mangle]
pub extern "C" fn mpa_channels(h: u32) -> u32 {
    SLOTS.with(|s| s.borrow().get(h as usize).and_then(Option::as_ref).map_or(0, |x| x.channels))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A silent MPEG-1 layer II frame: 48 kHz, 192 kbit/s, stereo → 576 bytes, 1152 samples per channel.
    fn silent_mp2_frame() -> Vec<u8> {
        let mut f = vec![0u8; 576];
        f[..4].copy_from_slice(&[0xFF, 0xFD, 0xA4, 0x04]); // sync, MPEG-1, layer II, no CRC; 192k, 48k; stereo
        f
    }

    #[test]
    fn decodes_mp2() {
        let mut slot = Slot::default();
        let pcm = decode(&mut slot, &silent_mp2_frame()).expect("layer II frame decodes");
        assert_eq!((slot.rate, slot.channels), (48000, 2));
        assert_eq!(pcm.len(), 1152 * 2);
        assert!(pcm.iter().all(|x| x.abs() < 1e-6));
        assert!(decode(&mut slot, &[0xFF, 0xFD, 0, 0]).is_none(), "truncated frame is skipped");
    }
}
