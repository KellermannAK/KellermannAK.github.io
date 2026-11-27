// AudioWorklet: runs the Rust engine (WebAssembly) on the audio thread.
// The main thread passes the .wasm bytes and the set seed via processorOptions.

const STATUS_LEN = 26;
// Clock for load measurement (performance.now is not available in every worklet scope).
const now = () => (globalThis.performance && performance.now ? performance.now() : Date.now());

class TechnoProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { wasm, seedLo, seedHi, gain, eco, genre, playlist } = options.processorOptions;
    this.ready = false;
    this.blocksPerStatus = Math.max(1, Math.round(sampleRate / 128 / 8)); // ~8 updates/s
    this.blockCount = 0;
    this.busyMs = 0;
    this.audioMs = 0;
    this.mem = null;
    WebAssembly.instantiate(wasm, {})
      .then(({ instance }) => {
        this.w = instance.exports;
        if (playlist && playlist.length && this.w.techno_new_playlist) {
          // Favourites: [seed, keyRoot, bpm] per song, played in order and wrapping around.
          this.w.techno_playlist_clear();
          for (const [s, root, bpm] of playlist) {
            this.w.techno_playlist_push(s >>> 0, Math.floor(s / 2 ** 32), root, bpm);
          }
          this.e = this.w.techno_new_playlist(sampleRate, eco ? 2 : 1, genre || 0);
        } else if (this.w.techno_new_genre) {
          this.e = this.w.techno_new_genre(seedLo >>> 0, seedHi >>> 0, sampleRate, eco ? 2 : 1, genre || 0);
        } else {
          this.e = eco
            ? this.w.techno_new_eco(seedLo >>> 0, seedHi >>> 0, sampleRate, 2)
            : this.w.techno_new(seedLo >>> 0, seedHi >>> 0, sampleRate);
        }
        this.w.techno_set_gain(this.e, gain);
        this.ready = true;
        this.port.postMessage({ type: "ready" });
      })
      .catch((err) => this.port.postMessage({ type: "error", message: String(err) }));
    this.stopped = false;
    this.port.onmessage = (ev) => {
      if (ev.data.type === "gain" && this.ready) this.w.techno_set_gain(this.e, ev.data.value);
      // Returning false from process() lets the browser destroy this processor; without
      // it a disconnected node keeps rendering forever.
      if (ev.data.type === "stop") this.stopped = true;
    };
  }

  // Typed-array views are cached and only rebuilt if wasm memory grew (old buffer detached).
  views(n) {
    const buf = this.w.memory.buffer;
    if (this.mem !== buf || this.n !== n) {
      this.mem = buf;
      this.n = n;
      this.left = new Float32Array(buf, this.w.techno_left(this.e), n);
      this.right = new Float32Array(buf, this.w.techno_right(this.e), n);
    }
  }

  process(_inputs, outputs) {
    if (this.stopped) {
      if (this.ready) this.w.techno_free(this.e);
      this.ready = false;
      return false;
    }
    const out = outputs[0];
    if (!this.ready || !out || out.length === 0) return true;
    const n = out[0].length;
    const t0 = now();
    this.w.techno_render(this.e, n);
    this.busyMs += now() - t0;
    this.audioMs += (n / sampleRate) * 1000;
    this.views(n);
    out[0].set(this.left);
    if (out[1]) out[1].set(this.right);
    if (++this.blockCount >= this.blocksPerStatus) {
      this.blockCount = 0;
      const ptr = this.w.techno_status(this.e);
      const st = new Float64Array(this.w.memory.buffer, ptr, STATUS_LEN).slice();
      // Share of the available real time the engine needed (100 % = dropouts).
      const load = this.audioMs > 0 ? this.busyMs / this.audioMs : 0;
      this.busyMs = 0;
      this.audioMs = 0;
      this.port.postMessage({ type: "status", status: st, load, sampleRate }, [st.buffer]);
    }
    return true;
  }
}

registerProcessor("techno", TechnoProcessor);
