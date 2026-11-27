// Audio codecs browsers lack, as mediabunny decoders: MP2 (DVB live TV) from our Rust core, AC-3/E-AC-3 and
// DTS from mediabunny's FFmpeg-in-wasm extensions.
import { AudioSample, CustomAudioDecoder, registerDecoder, type AudioCodec, type EncodedPacket } from 'mediabunny';
import { registerAc3Decoder } from '@mediabunny/ac3';
import { registerDtsDecoder } from '@mediabunny/dts';
import { Mpa } from './core';

// mediabunny reports every MPEG audio layer as 'mp3'; the Rust decoder takes all three.
class MpaDecoder extends CustomAudioDecoder {
  private mpa!: Mpa;
  static override supports(codec: AudioCodec) { return codec === 'mp3'; }
  init() { this.mpa = new Mpa(); }
  decode(packet: EncodedPacket) {
    const out = this.mpa.decode(packet.data);
    if (!out) return;
    this.onSample(new AudioSample({
      data: out.pcm, format: 'f32', numberOfChannels: out.channels, sampleRate: out.rate, timestamp: packet.timestamp,
    }));
  }
  flush() {}
  close() { this.mpa.close(); }
}

registerDecoder(MpaDecoder);
registerAc3Decoder();
registerDtsDecoder();
