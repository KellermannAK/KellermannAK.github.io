// Live set controller (main thread): audio context, worklet, wake lock, status display.

const $ = (id) => document.getElementById(id);
const NOTES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const keyName = (root, scale) =>
  `${NOTES[root]} ${["minor", "phrygian", "harmonic minor", "dorian", "major"][scale] || ""}`.trim();
const genre = () => (window.TechnoGenre ? window.TechnoGenre.current() : { id: "techno", name: "Techno", index: 0 });
// Genre the running set was started with (the switcher may change it later).
let setGenre = genre();
// Favourites mix: [[seed, keyRoot, bpm], ...] while one is playing, else null.
let favMix = null;
const SECTIONS = ["Intro", "Groove", "Build-up", "Breakdown", "Drop", "Outro"];
const TRANSITIONS = ["Hard cut", "Fast blend", "Slow blend", "Filter blend"];
// Short silent WAV: playing it through an <audio> element moves iOS into the
// "playback" audio session, so Web Audio is not muted by the ring/silent switch.
const SILENCE =
  "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQAAAAA=";

let ctx = null;
let node = null;
let wasmBytes = null;
let wakeLock = null;
let seed = 0;
let playing = false;
let silentAudio = null;
let eco = false;
let speed = 0; // browser speed test result (x realtime)
const ECO_KEY = "techno-eco"; // "auto" | "on" | "off"

function ecoPref() {
  try { return localStorage.getItem(ECO_KEY) || "auto"; } catch (_) { return "auto"; }
}

/**
 * Quarter-second benchmark of the engine on the main thread. Some browsers run
 * WebAssembly without their optimising compiler (e.g. Edge "enhanced security"), which
 * makes the engine 20-40x slower; the economy mode halves the load.
 */
// Main-thread engine instance, kept for naming songs (title / artist).
let mainWasm = null;

function songName(songSeed) {
  if (!mainWasm || !mainWasm.techno_name) return null;
  const lo = songSeed >>> 0;
  const hi = Math.floor(songSeed / 2 ** 32);
  const len = mainWasm.techno_name_genre
    ? mainWasm.techno_name_genre(lo, hi, setGenre.index)
    : mainWasm.techno_name(lo, hi);
  const bytes = new Uint8Array(mainWasm.memory.buffer, mainWasm.techno_name_ptr(), len);
  const [artist, title] = new TextDecoder().decode(bytes).split("\n");
  return { artist, title };
}

async function speedTest(bytes) {
  try {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const w = instance.exports;
    mainWasm = w;
    const e = w.techno_new(1, 0, 48000);
    for (let i = 0; i < 20; i++) w.techno_render(e, 128); // warm-up
    const frames = 12000;
    const t0 = performance.now();
    for (let i = 0; i < frames / 128; i++) w.techno_render(e, 128);
    const ms = performance.now() - t0;
    w.techno_free(e);
    return (frames / 48000) * 1000 / Math.max(ms, 0.01);
  } catch (_) {
    return 0;
  }
}

const mmss = (t) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;

function randomSeed() {
  const a = new Uint32Array(1);
  crypto.getRandomValues(a);
  return a[0] % 1000000;
}

function initialSeed() {
  const q = new URLSearchParams(location.search).get("set");
  const n = q === null ? NaN : parseInt(q, 10);
  return Number.isFinite(n) && n >= 0 ? n : randomSeed();
}

async function requestWakeLock() {
  try {
    if ("wakeLock" in navigator && document.visibilityState === "visible") {
      wakeLock = await navigator.wakeLock.request("screen");
    }
  } catch (_) { /* not supported or denied: playback still works */ }
}

function releaseWakeLock() {
  if (wakeLock) wakeLock.release().catch(() => {});
  wakeLock = null;
}

document.addEventListener("visibilitychange", () => {
  if (playing && document.visibilityState === "visible") requestWakeLock();
});

const PLAY_LABELS = { idle: "Start live", loading: "Loading…", playing: "Pause", paused: "Resume", stopped: "Start live" };

function setPlayButton(state) {
  const btn = $("lv-play");
  btn.dataset.state = state;
  btn.querySelector("span").textContent = PLAY_LABELS[state];
  btn.disabled = state === "loading";
}

async function ensureContext() {
  if (!ctx) {
    // Always render at 48 kHz: the browser resamples to the sound card. PCs often run at
    // 96 or 192 kHz, which would multiply the engine's CPU load by 2-4.
    const AC = window.AudioContext || window.webkitAudioContext;
    try {
      ctx = new AC({ latencyHint: "playback", sampleRate: 48000 });
    } catch (_) {
      ctx = new AC({ latencyHint: "playback" });
    }
    if (navigator.audioSession) {
      try { navigator.audioSession.type = "playback"; } catch (_) {}
    }
  }
  if (ctx.state !== "running") await ctx.resume();
  if (!silentAudio) {
    silentAudio = new Audio(SILENCE);
    silentAudio.loop = true;
    silentAudio.setAttribute("playsinline", "");
  }
  silentAudio.play().catch(() => {});
}

let starting = false;

async function startSet(newSeed, playlist = null) {
  if (starting) return; // ignore double clicks while loading
  starting = true;
  try {
    favMix = playlist;
    await startSetInner(newSeed);
  } finally {
    starting = false;
  }
}

async function startSetInner(newSeed) {
  setPlayButton("loading");
  await ensureContext(); // must be called first inside the click handler (iOS)
  if (!wasmBytes) {
    const [bytes] = await Promise.all([
      fetch("app/techno.wasm").then((r) => {
        if (!r.ok) throw new Error("techno.wasm is missing");
        return r.arrayBuffer();
      }),
      ctx.audioWorklet.addModule("app/worklet.js"),
    ]);
    wasmBytes = bytes;
    speed = await speedTest(wasmBytes);
  }
  const pref = ecoPref();
  eco = pref === "on" || (pref === "auto" && speed > 0 && speed < 8);
  if (window.TechnoStats) window.TechnoStats.setMeta({ wasm_speed: speed || null, eco });
  const hint = $("lv-slow");
  if (speed > 0 && speed < 4) {
    hint.hidden = false;
    hint.textContent = `Your browser runs the engine very slowly (${speed.toFixed(1)}× real time, normal is 30–50×). `
      + "In Microsoft Edge this is usually caused by “Enhanced security”: Settings → Privacy, search and services → "
      + "“Enhance your security on the web” → add an exception for this site. Or use Chrome/Firefox. "
      + (eco ? "Economy mode is on." : "");
  } else {
    hint.hidden = true;
  }
  if (node) {
    // Stop the old engine for good (a merely disconnected worklet keeps running).
    node.port.postMessage({ type: "stop" });
    node.disconnect();
    node.port.onmessage = null;
    node = null;
  }
  seed = newSeed;
  setGenre = genre();
  lastSongSeed = -1;
  const url = new URL(location.href);
  if (favMix) url.searchParams.delete("set");
  else url.searchParams.set("set", seed);
  history.replaceState(null, "", url);
  node = new AudioWorkletNode(ctx, "techno", {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [2],
    processorOptions: { wasm: wasmBytes, seedLo: seed, seedHi: 0, gain: volume(), eco, genre: setGenre.index, playlist: favMix },
  });
  node.port.onmessage = (ev) => {
    const m = ev.data;
    if (m.type === "status") {
      showStatus(m.status);
      showDiag(m.load, m.sampleRate);
    }
    else if (m.type === "ready") {
      playing = true;
      setPlayButton("playing");
      $("lv-info").hidden = false;
    } else if (m.type === "error") showError(m.message);
  };
  node.connect(ctx.destination);
  requestWakeLock();
  setupMediaSession();
}

function volume() {
  const v = parseFloat($("lv-vol").value);
  return v * v; // perceptual curve
}

// Smoothed engine load for the diagnostics line.
let loadAvg = 0;
function showDiag(load, rate) {
  loadAvg = loadAvg ? loadAvg * 0.8 + load * 0.2 : load;
  const ua = navigator.userAgent;
  const browser = (/(Edg|Firefox|OPR|Chrome|Safari)\/(\d+)/.exec(ua) || [])
    .slice(1, 3).join(" ").replace("Edg", "Edge").replace("OPR", "Opera");
  const out = ctx ? Math.round(ctx.sampleRate) : rate;
  // Technical diagnostics only on request (?debug); the page does not show statistics.
  $("lv-diag").hidden = !new URLSearchParams(location.search).has("debug");
  $("lv-diag").textContent =
    `Engine load ${Math.round(loadAvg * 100)} % · ${rate} Hz${out !== rate ? ` (output ${out} Hz)` : ""}`
    + `${eco ? " · economy mode (internal 24 kHz)" : ""} · browser test ${speed ? speed.toFixed(0) + "×" : "?"} · ${browser || "browser?"}`;
}

function showError(msg) {
  $("lv-error").textContent = `Error: ${msg}`;
  $("lv-error").hidden = false;
  setPlayButton("stopped");
}

let lastTitle = "";
let lastStatus = null;
let lastSongSeed = -1;
let currentName = null;

function feedback(rating) {
  if (!lastStatus || !window.TechnoFeedback) return;
  const [song, songSeed, root, scale, camelot, bpm, section, bar, secBar, secBars, totalBars] = lastStatus;
  window.TechnoFeedback.open({
    source: "Live set",
    favorites: !!favMix,
    genre: setGenre.id,
    setSeed: seed,
    songIndex: song,
    songSeed,
    key: keyName(root, scale),
    title: currentName ? `${currentName.artist} – ${currentName.title}` : undefined,
    camelot: `${camelot}${ring()}`,
    bpm,
    section: SECTIONS[section],
    sectionBar: `${secBar + 1}/${secBars}`,
    songBar: `${bar + 1}/${totalBars}`,
    songBarNum: bar + 1,
    time: mmss(lastStatus[15]),
  }, rating);
}

// Camelot ring from the status (index 16; older engines only had the minor "A" ring).
const ring = () => (lastStatus && lastStatus[16] === 1 ? "B" : "A");

function showStatus(s) {
  lastStatus = s;
  const [song, songSeed, root, scale, camelot, bpm, section, bar, secBar, secBars, totalBars, energy,
    transition, barsTo, inTransition, seconds] = s;
  const key = keyName(root, scale);
  if (songSeed !== lastSongSeed) {
    lastSongSeed = songSeed;
    currentName = songName(songSeed);
  }
  if (window.TechnoStats) {
    window.TechnoStats.setNow({
      source: "live",
      genre: setGenre.id,
      source_seed: seed,
      song_seed: songSeed,
      song_index: song,
      title: currentName ? currentName.title : null,
      artist: currentName ? currentName.artist : null,
    });
  }
  $("lv-song").textContent = currentName ? currentName.title : `Song ${song + 1}`;
  $("lv-artist").textContent = currentName ? currentName.artist : "";
  $("lv-key").textContent = `Song ${song + 1} · ${key} · ${bpm} BPM`;
  $("lv-cam").textContent = `${camelot}${ring()}`;
  $("lv-section").textContent = SECTIONS[section] || "–";
  $("lv-bars").textContent = `Bar ${secBar + 1}/${secBars}`;
  $("lv-progress").style.width = `${Math.min(100, ((secBar + 1) / Math.max(1, secBars)) * 100)}%`;
  $("lv-energy").style.width = `${Math.round(energy * 100)}%`;
  $("lv-songbar").textContent = `Song bar ${bar + 1}/${totalBars} · Seed ${songSeed}`;
  let next = "";
  if (inTransition) next = `Transition running: ${TRANSITIONS[transition] || ""}`;
  else if (transition >= 0) next = `Next song: ${TRANSITIONS[transition].toLowerCase()} in ${barsTo} bars`;
  $("lv-next").textContent = next;
  $("lv-time").textContent = `${favMix ? "Favorites mix" : `Set #${seed}`} · ${mmss(seconds)}`;
  showFav();
  $("lv-card-title").textContent = favMix ? "Favorites mix" : "Endless set";
  const title = currentName ? currentName.title : `Song ${song + 1} – ${key}`;
  if (title !== lastTitle && "mediaSession" in navigator) {
    lastTitle = title;
    navigator.mediaSession.metadata = new MediaMetadata({
      title,
      artist: currentName ? currentName.artist : "Techno Generator",
      album: `Techno Generator · ${setGenre.name} · ${favMix ? "Favorites mix" : `Live set #${seed}`}`,
    });
  }
}

/** Heart of the playing song: info for the favourites list, from the last status. */
function favInfo() {
  if (!lastStatus) return null;
  const [, songSeed, root, scale, , bpm] = lastStatus;
  return {
    genre: setGenre.id, seed: songSeed, root, bpm,
    title: currentName ? currentName.title : "", artist: currentName ? currentName.artist : "",
    key: keyName(root, scale),
  };
}

function showFav() {
  const b = $("lv-fav");
  const f = window.TechnoFavs;
  if (!b || !f || !lastStatus) return;
  const on = f.has(setGenre.id, lastStatus[1]);
  b.classList.toggle("on", on);
  b.setAttribute("aria-pressed", String(on));
  b.setAttribute("aria-label", on ? "Remove from favorites" : "Add to favorites");
}

/** Play the favourites of a genre as a mix (in order, endless). */
async function playFavorites(genreId) {
  const g = (window.TechnoGenre ? window.TechnoGenre.list : []).find((x) => x.id === genreId);
  const favs = window.TechnoFavs ? window.TechnoFavs.forGenre(genreId) : [];
  if (!g || !favs.length) return;
  // Oldest first: the order in which they were added.
  const pl = favs.slice().reverse().map((f) => [f.seed, f.root, f.bpm]);
  if (window.TechnoGenre) window.TechnoGenre.set(genreId);
  $("lv-play").closest(".card").scrollIntoView({ behavior: "smooth", block: "start" });
  await startSet(0, pl).catch((e) => showError(e.message));
}
window.TechnoLive = { playFavorites };

function setupMediaSession() {
  if (!("mediaSession" in navigator)) return;
  navigator.mediaSession.setActionHandler("play", () => togglePlay(true));
  navigator.mediaSession.setActionHandler("pause", () => togglePlay(false));
}

async function togglePlay(wantPlay) {
  if (!node) return startSet(seed || initialSeed());
  if (wantPlay === undefined) wantPlay = !playing;
  if (wantPlay) {
    await ensureContext();
    playing = true;
    requestWakeLock();
    setPlayButton("playing");
  } else {
    await ctx.suspend();
    if (silentAudio) silentAudio.pause();
    playing = false;
    releaseWakeLock();
    setPlayButton("paused");
  }
}

function init() {
  if (!window.AudioWorkletNode) {
    showError("This browser does not support AudioWorklet.");
    $("lv-play").disabled = true;
    return;
  }
  seed = initialSeed();
  $("lv-seed").value = seed;
  $("lv-play").addEventListener("click", () => togglePlay().catch((e) => showError(e.message)));
  $("lv-new").addEventListener("click", () => {
    const s = randomSeed();
    $("lv-seed").value = s;
    startSet(s).catch((e) => showError(e.message));
  });
  $("lv-seed-go").addEventListener("click", () => {
    const s = parseInt($("lv-seed").value, 10);
    if (Number.isFinite(s) && s >= 0) startSet(s).catch((e) => showError(e.message));
  });
  $("lv-fav").addEventListener("click", () => {
    const i = favInfo();
    if (i && window.TechnoFavs) window.TechnoFavs.toggle(i);
    showFav();
  });
  window.addEventListener("favchange", showFav);
  $("lv-up").addEventListener("click", () => feedback("up"));
  $("lv-down").addEventListener("click", () => feedback("down"));
  $("lv-idea").addEventListener("click", () => feedback("idea"));
  const ecoSel = $("lv-eco");
  ecoSel.value = ecoPref();
  ecoSel.addEventListener("change", () => {
    try { localStorage.setItem(ECO_KEY, ecoSel.value); } catch (_) {}
    if (node) startSet(seed, favMix).catch((e) => showError(e.message)); // restart with the new mode
  });
  $("lv-vol").addEventListener("input", () => {
    if (node) node.port.postMessage({ type: "gain", value: volume() });
  });
  // Pause other players when the live set starts and vice versa.
  document.addEventListener("play", () => { if (playing) togglePlay(false); }, true);
  // Genre switch: a running set continues in the new genre (same set seed); a paused or
  // stopped one is reset so the next start plays the new genre.
  window.addEventListener("genrechange", () => {
    if (favMix && node) {
      // A favourites mix belongs to one genre: stop it (the new genre may have none).
      favMix = null;
      node.port.postMessage({ type: "stop" });
      node.disconnect();
      node.port.onmessage = null;
      node = null;
      playing = false;
      lastStatus = null;
      $("lv-info").hidden = true;
      setPlayButton("idle");
      if (ctx && ctx.state === "suspended") ctx.resume().catch(() => {});
    } else if (node && playing) {
      startSet(seed).catch((e) => showError(e.message));
    } else if (node) {
      node.port.postMessage({ type: "stop" });
      node.disconnect();
      node.port.onmessage = null;
      node = null;
      playing = false;
      lastStatus = null;
      $("lv-info").hidden = true;
      setPlayButton("idle");
      if (ctx && ctx.state === "suspended") ctx.resume().catch(() => {});
    }
  });
}

init();
