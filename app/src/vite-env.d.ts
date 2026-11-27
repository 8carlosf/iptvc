/// <reference types="svelte" />
/// <reference types="vite/client" />
declare module '*.wasm?init' {
  const init: (imports?: WebAssembly.Imports) => Promise<WebAssembly.Instance>;
  export default init;
}
interface HTMLVideoElement {
  /** Chrome: bytes of audio decoded so far. */
  readonly webkitAudioDecodedByteCount?: number;
}
