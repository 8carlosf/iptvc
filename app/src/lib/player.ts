// Display helpers: whether a file plays as-is in <video>, and a WebGL screen that can deinterlace (browsers show
// 1080i fields combed), fed from <video> or from the software decoder's frames.
import { ALL_FORMATS, Input, UrlSource } from 'mediabunny';

const VS = `attribute vec2 p; varying vec2 uv; void main() { uv = vec2(p.x + 1.0, 1.0 - p.y) * 0.5; gl_Position = vec4(p, 0.0, 1.0); }`;
// Deinterlace by linear blend: each line mixed with its neighbours, so the two fields' combing averages out.
const FS = `precision mediump float; uniform sampler2D t; uniform float dy; uniform bool di; varying vec2 uv;
void main() {
  vec4 c = texture2D(t, uv);
  if (di) c = 0.5 * c + 0.25 * (texture2D(t, uv - vec2(0.0, dy)) + texture2D(t, uv + vec2(0.0, dy)));
  gl_FragColor = c;
}`;

/** A WebGL canvas that shows frames, optionally deinterlaced. */
export class Screen {
  deinterlace = false;
  private gl: WebGLRenderingContext;
  private dy: WebGLUniformLocation | null;
  private di: WebGLUniformLocation | null;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl', { alpha: false })!;
    const prog = gl.createProgram()!;
    for (const [type, src] of [[gl.VERTEX_SHADER, VS], [gl.FRAGMENT_SHADER, FS]] as const) {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      gl.attachShader(prog, s);
    }
    gl.linkProgram(prog);
    gl.useProgram(prog);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
    for (const k of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T]) gl.texParameteri(gl.TEXTURE_2D, k, gl.CLAMP_TO_EDGE);
    for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.LINEAR);
    this.gl = gl;
    this.dy = gl.getUniformLocation(prog, 'dy');
    this.di = gl.getUniformLocation(prog, 'di');
  }

  draw(src: TexImageSource, width: number, height: number) {
    const { gl, canvas } = this;
    if (!width) return;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    }
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src);
    gl.uniform1f(this.dy, 1 / height);
    gl.uniform1i(this.di, this.deinterlace ? 1 : 0);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }
}

/** Mirrors `video`'s frames onto `screen` until the returned function is called. */
export function mirror(video: HTMLVideoElement, screen: Screen): () => void {
  let id = 0;
  const draw = () => {
    screen.draw(video, video.videoWidth, video.videoHeight);
    id = video.requestVideoFrameCallback(draw);
  };
  id = video.requestVideoFrameCallback(draw);
  return () => video.cancelVideoFrameCallback(id);
}

/** Whether <video> can play this file as-is (it needs no CORS); `remux` if it needs our pipeline. */
export async function route(url: string): Promise<'native' | 'remux'> {
  const input = new Input({ source: new UrlSource(url), formats: ALL_FORMATS });
  try {
    return document.createElement('video').canPlayType(await input.getMimeType()) ? 'native' : 'remux';
  } catch {
    return 'native'; // unreadable here (most often: no CORS on the file), so let <video> try
  } finally {
    input.dispose();
  }
}
