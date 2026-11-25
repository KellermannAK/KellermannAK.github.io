// Usage statistics: page views, time on the page and listening time per song (10 s steps).
//
// What is measured: seconds the page is visible or music is playing, and for every song the
// seconds it was heard (live set or mix). What is sent: a random visit id, those seconds, song
// seeds / titles / artists, device class, browser family, engine version, build and the browser
// speed test result. Nothing else; no IP address is stored by these tables.
//
// Off switch: the account card, or the browser's "Do Not Track" / Global Privacy Control.
//
// Sources tell us what is playing with setNow(); "playing" means the last report is fresh
// (live status arrives ~8x per second, an <audio> element's timeupdate ~4x per second while it
// plays), so pausing needs no special handling.
import * as DB from "./db.js";

const cfg = window.TECHNO_CONFIG || {};
const TICK_MS = 1000;
const FLUSH_MS = 30000;
const FRESH_MS = 2500;
const KEY = "techno-stats"; // "on" | "off" | unset (= follow Do Not Track)

const read = (k) => {
  try { return localStorage.getItem(k); } catch (_) { return null; }
};

/** "on" / "off" as chosen by the user, or "auto" (follow the browser's privacy signals). */
export function mode() {
  const v = read(KEY);
  return v === "on" || v === "off" ? v : "auto";
}

export function enabled() {
  if (!DB.enabled) return false;
  const m = mode();
  if (m !== "auto") return m === "on";
  return !(navigator.doNotTrack === "1" || navigator.globalPrivacyControl === true);
}

export function setMode(m) {
  try {
    if (m === "auto") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, m);
  } catch (_) { /* storage unavailable */ }
}

const uuid = () =>
  crypto.randomUUID
    ? crypto.randomUUID()
    : "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
        const r = (Math.random() * 16) | 0;
        return (c === "x" ? r : (r & 3) | 8).toString(16);
      });

const visit = uuid();
const songs = new Map(); // key -> { ...info, ms }
let pageMs = 0;
let now = null;
let nowAt = 0;
let dirty = true; // the first flush counts the page view
let started = false;
let meta = { build: null, engine: null, wasm_speed: null, eco: null };

const ua = navigator.userAgent;
const device = /iPad|Tablet/i.test(ua) ? "tablet" : /Mobi|Android|iPhone/i.test(ua) ? "mobile" : "desktop";
const browser = (() => {
  const m = /(Edg|OPR|Firefox|Chrome|Safari)\/(\d+)/.exec(ua);
  if (!m) return null;
  const names = { Edg: "Edge", OPR: "Opera" };
  return `${names[m[1]] || m[1]} ${m[2]}`;
})();

const floor10 = (ms) => Math.floor(ms / 10000) * 10;

/** A source reports what is playing right now (call it often while playing). */
export function setNow(info) {
  now = info;
  nowAt = performance.now();
}

/** Extra fields for the visit row (speed test, economy mode, build). */
export function setMeta(m) {
  meta = { ...meta, ...m };
  dirty = true;
}

let last = performance.now();
function tick() {
  const t = performance.now();
  const dt = Math.min(t - last, 2000);
  last = t;
  if (!enabled()) return;
  const listening = now && t - nowAt < FRESH_MS;
  if (listening) {
    const key = `${now.genre || "techno"}|${now.source}|${now.source_seed}|${now.song_seed}`;
    const s = songs.get(key) || { ...now, ms: 0 };
    s.ms += dt;
    s.title = now.title || s.title;
    s.artist = now.artist || s.artist;
    songs.set(key, s);
    dirty = true;
  }
  if (listening || document.visibilityState === "visible") {
    pageMs += dt;
    dirty = true;
  }
}

async function flush() {
  if (!enabled() || !dirty || !started) return;
  const token = DB.accessToken();
  if (!token) return;
  const listens = [...songs.values()]
    .slice(-60)
    .map((s) => ({
      genre: s.genre || "techno",
      source: s.source,
      source_seed: s.source_seed,
      song_seed: s.song_seed,
      song_index: s.song_index || 0,
      title: s.title || null,
      artist: s.artist || null,
      seconds: floor10(s.ms),
    }));
  dirty = false;
  try {
    // keepalive lets the request finish while the page is being closed. Values are cumulative,
    // so a retry or a late duplicate can never double count.
    const res = await fetch(`${cfg.supabaseUrl}/rest/v1/rpc/track`, {
      method: "POST",
      keepalive: true,
      headers: {
        apikey: cfg.supabaseAnonKey,
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        p_visit: visit,
        p_page_seconds: floor10(pageMs),
        p_listens: listens,
        p_device: device,
        p_browser: browser,
        p_engine: meta.engine,
        p_build: meta.build,
        p_wasm_speed: meta.wasm_speed,
        p_eco: meta.eco,
      }),
    });
    if (!res.ok) dirty = true;
  } catch (_) {
    dirty = true;
  }
}

async function start() {
  if (!DB.enabled) return;
  try {
    const info = await fetch("songs.json", { cache: "no-store" }).then((r) => r.json());
    meta.build = (info.commit || "").slice(0, 7) || null;
    meta.engine = (info.version && info.version.engine) || null;
  } catch (_) { /* build info is optional */ }
  await DB.db(); // creates the anonymous session
  started = true;
  last = performance.now();
  setInterval(tick, TICK_MS);
  setInterval(flush, FLUSH_MS);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flush();
  });
  addEventListener("pagehide", flush);
  flush(); // page view
}

window.TechnoStats = { setNow, setMeta, mode, setMode, enabled };
start();
