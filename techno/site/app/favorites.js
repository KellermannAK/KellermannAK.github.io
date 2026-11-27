// Favourite songs: a heart on the playing song and in every mix tracklist, and a personal
// list (per genre) that can be played as a mix. The list lives on this device (works without
// an account and offline) and is mirrored in the database when it is available, so it follows
// a registered user to other devices. Songs are stored as seed + the key and tempo they were
// heard in; the engine generates them again when they are played.
import * as DB from "./db.js";

const KEY = "techno-favs-v1";
const DEL = "techno-favs-del-v1";
const USER = "techno-favs-user";
const MAX = 300;

const readJson = (k, d) => {
  try { return JSON.parse(localStorage.getItem(k) || "") || d; } catch (_) { return d; }
};
const write = (k, v) => {
  try { localStorage.setItem(k, JSON.stringify(v)); } catch (_) { /* storage unavailable */ }
};

let list = readJson(KEY, []); // { genre, seed, root, bpm, title, artist, key, build, at, synced }
let dels = readJson(DEL, []); // { genre, seed } deleted locally, not yet in the database
let build = "";

fetch("songs.json", { cache: "no-store" })
  .then((r) => r.json())
  .then((j) => { build = (j.commit || "").slice(0, 7); })
  .catch(() => {});

const same = (a, g, s) => a.genre === g && a.seed === s;
const emit = () => window.dispatchEvent(new CustomEvent("favchange"));
const persist = () => { write(KEY, list); write(DEL, dels); };

export const has = (genre, seed) => list.some((f) => same(f, genre, seed));
/** Favourites of one genre, newest first. */
export const forGenre = (genre) => list.filter((f) => f.genre === genre).sort((a, b) => (a.at < b.at ? 1 : -1));

const row = (f) => ({
  genre: f.genre, song_seed: f.seed, key_root: f.root, bpm: f.bpm,
  title: f.title || null, artist: f.artist || null, key: f.key || null, build: f.build || null,
});

async function client() {
  if (!DB.enabled) return null;
  const c = await DB.db();
  return c && DB.accessToken() ? c : null;
}

async function pushAdd(f) {
  const c = await client();
  if (!c) return;
  const { error } = await c.from("favorites").insert(row(f));
  if (!error || error.code === "23505") { f.synced = true; persist(); }
}

async function pushDelete(g, s) {
  const c = await client();
  if (!c) return;
  const { error } = await c.from("favorites").delete().eq("genre", g).eq("song_seed", s);
  if (!error) { dels = dels.filter((d) => !same(d, g, s)); persist(); }
}

/** Add or remove a song. `info`: { genre, seed, root, bpm, title, artist, key, build }. */
export function toggle(info) {
  const { genre, seed } = info;
  if (!Number.isFinite(seed) || !Number.isFinite(info.root) || !Number.isFinite(info.bpm)) return false;
  if (has(genre, seed)) {
    list = list.filter((f) => !same(f, genre, seed));
    if (!dels.some((d) => same(d, genre, seed))) dels.push({ genre, seed });
    persist();
    emit();
    pushDelete(genre, seed);
    return false;
  }
  if (list.length >= MAX) return null;
  const f = {
    genre, seed, root: ((info.root % 12) + 12) % 12, bpm: Math.round(info.bpm),
    title: info.title || "", artist: info.artist || "", key: info.key || "",
    build: info.build || build, at: new Date().toISOString(), synced: false,
  };
  list.push(f);
  dels = dels.filter((d) => !same(d, genre, seed));
  persist();
  emit();
  pushAdd(f);
  return true;
}

export const remove = (genre, seed) => (has(genre, seed) ? toggle({ genre, seed, root: 0, bpm: 120 }) : false);

/** Reconcile with the database: send pending changes, then take the server's list. */
export async function sync() {
  const c = await client();
  if (!c) return;
  // A different account than before (sign-in on this device): upload what is here as well.
  const { data: u } = await c.auth.getUser();
  const uid = u && u.user ? u.user.id : null;
  const prev = readJson(USER, null);
  if (uid && prev && prev !== uid) list.forEach((f) => { f.synced = false; });
  if (uid) write(USER, uid);
  for (const d of [...dels]) await pushDelete(d.genre, d.seed);
  for (const f of list.filter((x) => !x.synced)) await pushAdd(f);
  const { data, error } = await c.from("favorites").select("*").order("created_at", { ascending: false }).limit(MAX);
  if (error) return;
  const remote = data
    .map((r) => ({
      genre: r.genre, seed: Number(r.song_seed), root: r.key_root, bpm: r.bpm,
      title: r.title || "", artist: r.artist || "", key: r.key || "", build: r.build || "",
      at: r.created_at, synced: true,
    }))
    .filter((r) => !dels.some((d) => same(d, r.genre, r.seed)));
  const pending = list.filter((f) => !f.synced && !remote.some((r) => same(r, f.genre, f.seed)));
  list = [...remote, ...pending];
  persist();
  emit();
}

let timer = 0;
const syncSoon = () => { clearTimeout(timer); timer = setTimeout(() => sync().catch(() => {}), 500); };

window.addEventListener("storage", (e) => {
  if (e.key === KEY) { list = readJson(KEY, []); emit(); }
});
DB.onAuth(syncSoon);
if (DB.enabled) DB.db().then(syncSoon);

window.TechnoFavs = { has, toggle, remove, forGenre, sync };
emit();
