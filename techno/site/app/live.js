// Live set controller (main thread): audio context, worklet, wake lock, status display.

const $ = (id) => document.getElementById(id);
const NOTES = ["C", "Cis", "D", "Es", "E", "F", "Fis", "G", "Gis", "A", "B", "H"];
const keyName = (root, scale) =>
  [`${NOTES[root]}-Moll`, `${NOTES[root]} phrygisch`, `${NOTES[root]} harmonisch Moll`][scale] || NOTES[root];
const SECTIONS = ["Intro", "Groove", "Build-up", "Breakdown", "Drop", "Outro"];
const TRANSITIONS = ["harter Cut", "schnelle Blende", "langsame Blende", "Filter-Blende"];
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

const PLAY_LABELS = { idle: "Live starten", loading: "Lädt…", playing: "Pause", paused: "Weiter", stopped: "Live starten" };

function setPlayButton(state) {
  const btn = $("lv-play");
  btn.dataset.state = state;
  btn.querySelector("span").textContent = PLAY_LABELS[state];
  btn.disabled = state === "loading";
}

async function ensureContext() {
  if (!ctx) {
    ctx = new (window.AudioContext || window.webkitAudioContext)({ latencyHint: "playback" });
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

async function startSet(newSeed) {
  setPlayButton("loading");
  await ensureContext(); // must be called first inside the click handler (iOS)
  if (!wasmBytes) {
    const [bytes] = await Promise.all([
      fetch("app/techno.wasm").then((r) => {
        if (!r.ok) throw new Error("techno.wasm fehlt");
        return r.arrayBuffer();
      }),
      ctx.audioWorklet.addModule("app/worklet.js"),
    ]);
    wasmBytes = bytes;
  }
  if (node) {
    node.disconnect();
    node.port.onmessage = null;
  }
  seed = newSeed;
  history.replaceState(null, "", `?set=${seed}`);
  node = new AudioWorkletNode(ctx, "techno", {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [2],
    processorOptions: { wasm: wasmBytes, seedLo: seed, seedHi: 0, gain: volume() },
  });
  node.port.onmessage = (ev) => {
    const m = ev.data;
    if (m.type === "status") showStatus(m.status);
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

function showError(msg) {
  $("lv-error").textContent = `Fehler: ${msg}`;
  $("lv-error").hidden = false;
  setPlayButton("stopped");
}

let lastTitle = "";
function showStatus(s) {
  const [song, songSeed, root, scale, camelot, bpm, section, bar, secBar, secBars, totalBars, energy,
    transition, barsTo, inTransition, seconds] = s;
  const key = keyName(root, scale);
  $("lv-song").textContent = `Song ${song + 1}`;
  $("lv-key").textContent = `${key} · ${bpm} BPM`;
  $("lv-cam").textContent = `${camelot}A`;
  $("lv-section").textContent = SECTIONS[section] || "–";
  $("lv-bars").textContent = `Takt ${secBar + 1}/${secBars}`;
  $("lv-progress").style.width = `${Math.min(100, ((secBar + 1) / Math.max(1, secBars)) * 100)}%`;
  $("lv-energy").style.width = `${Math.round(energy * 100)}%`;
  $("lv-songbar").textContent = `Song-Takt ${bar + 1}/${totalBars} · Seed ${songSeed}`;
  let next = "";
  if (inTransition) next = `Übergang läuft: ${TRANSITIONS[transition] || ""}`;
  else if (transition >= 0) next = `Nächster Song: ${TRANSITIONS[transition]} in ${barsTo} Takten`;
  $("lv-next").textContent = next;
  $("lv-time").textContent = `Set #${seed} · ${mmss(seconds)}`;
  const title = `Song ${song + 1} – ${key}`;
  if (title !== lastTitle && "mediaSession" in navigator) {
    lastTitle = title;
    navigator.mediaSession.metadata = new MediaMetadata({ title, artist: "Techno Generator", album: `Live-Set #${seed}` });
  }
}

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
    showError("Dieser Browser unterstützt kein AudioWorklet.");
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
  $("lv-vol").addEventListener("input", () => {
    if (node) node.port.postMessage({ type: "gain", value: volume() });
  });
  // Pause other players when the live set starts and vice versa.
  document.addEventListener("play", () => { if (playing) togglePlay(false); }, true);
}

init();
