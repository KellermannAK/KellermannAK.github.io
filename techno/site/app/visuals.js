// Visuals mode: hides everything below the genre switcher and shows two turntables with a
// mixer in between. The records spin while their deck is audible, the artist and the title
// run around the record label, and the mixer shows what the set engine does: channel faders,
// low-band kills, the playing song's BPM and key, the arrangement energy and the next
// transition. All data comes from the live set (`TechnoLive.visual()`); the buttons in the
// mixer press the real controls of the live card.
import { labelSvg } from "./labels.js";

const KEY = "techno-visuals";
const SPEED = 200; // degrees per second: 33 1/3 rpm
const LEDS = 10;

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const read = () => {
  try { return localStorage.getItem(KEY) === "1"; } catch (_) { return false; }
};

// ------------------------------------------------------------------ turntable drawing

const fmt = (t) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;

const pt = (cx, cy, r, deg) => {
  const a = (deg * Math.PI) / 180;
  return [cx + r * Math.sin(a), cy - r * Math.cos(a)];
};
const f = (n) => n.toFixed(2);

function deckSvg(i) {
  const [c, R] = [110, 98];
  const grooves = [90, 83, 76, 69, 63].map((r) => `<circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="#202026" stroke-width="0.9"/>`).join("");
  return `
  <svg viewBox="0 0 220 220" class="tt" role="img" aria-label="Turntable ${i ? "B" : "A"}">
    <rect x="2" y="2" width="216" height="216" rx="16" class="tt-body"/>
    <circle cx="${c}" cy="${c}" r="104" class="tt-platter"/>
    <g class="rec" id="rec-${i}">
      <circle cx="${c}" cy="${c}" r="${R}" fill="#0d0d10"/>
      ${grooves}
      <path d="M${c} ${c} L${c} ${c - R} A${R} ${R} 0 0 1 ${f(c + R * Math.sin(0.5))} ${f(c - R * Math.cos(0.5))} Z" fill="#fff" opacity="0.06"/>
      <path d="M${c} ${c} L${c} ${c + R} A${R} ${R} 0 0 1 ${f(c - R * Math.sin(0.5))} ${f(c + R * Math.cos(0.5))} Z" fill="#fff" opacity="0.06"/>
      <g id="lab-${i}"><circle cx="${c}" cy="${c}" r="58" class="tt-label"/></g>
      <circle cx="${c}" cy="${c}" r="3.2" fill="#e6e6e6"/>
    </g>
    <g class="arm" id="arm-${i}">
      <circle cx="196" cy="30" r="10" class="tt-pivot"/>
      <line x1="196" y1="30" x2="170" y2="148" class="tt-tube"/>
      <rect x="160" y="144" width="15" height="24" rx="3" transform="rotate(14 170 148)" class="tt-head"/>
    </g>
  </svg>`;
}

// ------------------------------------------------------------------ build

let built = false;
let source = "live"; // what drives the turntables: the live set or a mix
let seeking = false;

/** Switch between the live set and a mix (the other one is paused). */
function chooseSource(s) {
  const MV = window.TechnoMixVisual;
  if (s === source) return;
  if (s === "mix") {
    if (!MV || !MV.count()) return;
    if ($("lv-play").dataset.state === "playing") $("lv-play").click(); // pause the live set
  } else if (MV && MV.isPlaying()) {
    MV.toggle(); // pause the mix
  }
  source = s;
}
const decks = [{ angle: 0, vel: 0, key: "" }, { angle: 0, vel: 0, key: "" }];

function build() {
  if (built) return;
  built = true;
  const box = $("visuals");
  box.innerHTML = `
    <div class="vis">
      <div class="vis-deck" data-i="0">${deckSvg(0)}<div class="vis-cap" id="cap-0"></div></div>
      <div class="vis-mixer" aria-label="Mixer">
        <div class="mx-src" role="group" aria-label="Source">
          <button id="src-live" aria-pressed="true">Live</button>
          <button id="src-mix" aria-pressed="false">Mix</button>
        </div>
        <div class="mx-lcd"><span><b id="mx-bpm">---</b> BPM</span><span><b id="mx-key">--</b></span></div>
        <div class="mx-sec" id="mx-sec">&nbsp;</div>
        <div class="mx-mid">
          <div class="mx-ch" data-i="0">
            <div class="mx-lows" id="lows-0">LOW</div>
            <div class="mx-fader"><div class="mx-fill" id="fill-0"></div><div class="mx-knob" id="knob-0"></div></div>
            <div class="mx-name">A</div>
          </div>
          <div class="mx-center">
            <div class="mx-leds" id="mx-leds">${'<i></i>'.repeat(LEDS)}</div>
            <div class="mx-transport">
              <button class="mx-skip" id="mx-sprev" aria-label="Previous song" hidden><svg viewBox="0 0 24 24"><path d="M6 5h2.5v14H6zM20 5v14L9.5 12z"/></svg></button>
              <button class="mx-play" id="mx-play" aria-label="Play">
                <svg class="ico-play" viewBox="0 0 24 24"><path d="M7 4.5v15l13-7.5z"/></svg>
                <svg class="ico-pause" viewBox="0 0 24 24"><path d="M6 4h4.5v16H6zM13.5 4H18v16h-4.5z"/></svg>
              </button>
              <button class="mx-skip" id="mx-snext" aria-label="Next song" hidden><svg viewBox="0 0 24 24"><path d="M15.5 5H18v14h-2.5zM4 5v14l10.5-7z"/></svg></button>
            </div>
            <input type="range" id="mx-vol" min="0" max="1" step="0.01" aria-label="Volume">
            <div class="mx-row">
              <button class="heart" id="mx-fav" aria-label="Add to favorites" aria-pressed="false"><svg viewBox="0 0 24 24"><path d="M12 20.4C5.2 15.2 3 12.1 3 8.9 3 6.2 5 4.5 7.3 4.5c1.9 0 3.5 1.1 4.7 3 1.2-1.9 2.8-3 4.7-3C19 4.5 21 6.2 21 8.9c0 3.2-2.2 6.3-9 11.5z"/></svg></button>
              <button class="rate" data-r="up" aria-label="Thumbs up">👍</button>
              <button class="rate" data-r="down" aria-label="Thumbs down">👎</button>
              <button class="rate" data-r="idea" aria-label="Idea">💡</button>
            </div>
            <button class="btn mx-new" id="mx-new">New set</button>
          </div>
          <div class="mx-ch" data-i="1">
            <div class="mx-lows" id="lows-1">LOW</div>
            <div class="mx-fader"><div class="mx-fill" id="fill-1"></div><div class="mx-knob" id="knob-1"></div></div>
            <div class="mx-name">B</div>
          </div>
        </div>
        <div class="mx-seek" id="mx-seekrow" hidden><span id="mx-t0">0:00</span><input type="range" id="mx-seek" min="0" max="1000" value="0" aria-label="Position in the mix"><span id="mx-t1">0:00</span></div>
        <div class="mx-mixbar" id="mx-mixbar" hidden><button id="mx-mprev" aria-label="Previous mix">◀</button><span id="mx-mname"></span><button id="mx-mnext" aria-label="Next mix">▶</button></div>
        <div class="mx-next" id="mx-next">&nbsp;</div>
      </div>
      <div class="vis-deck" data-i="1">${deckSvg(1)}<div class="vis-cap" id="cap-1"></div></div>
    </div>`;
  const vol = $("mx-vol");
  vol.value = $("lv-vol").value;
  vol.addEventListener("input", () => {
    if (window.TechnoMixVisual) window.TechnoMixVisual.setVolume(vol.value);
    $("lv-vol").value = vol.value;
    $("lv-vol").dispatchEvent(new Event("input", { bubbles: true }));
  });
  const MV = () => window.TechnoMixVisual;
  $("mx-play").addEventListener("click", () => {
    if (source === "mix" && MV()) MV().toggle();
    else $("lv-play").click();
  });
  $("mx-new").addEventListener("click", () => $("lv-new").click());
  $("mx-fav").addEventListener("click", () => {
    if (source === "mix" && MV()) MV().toggleFavorite();
    else if (window.TechnoLive) window.TechnoLive.toggleFavorite();
  });
  // Like / dislike / idea for the moment that is playing (opens the feedback dialog).
  document.querySelectorAll(".rate").forEach((b) => b.addEventListener("click", () => {
    if (source === "mix" && MV()) MV().feedback(b.dataset.r);
    else if (window.TechnoLive) window.TechnoLive.feedback(b.dataset.r);
  }));
  $("src-live").addEventListener("click", () => chooseSource("live"));
  $("src-mix").addEventListener("click", () => chooseSource("mix"));
  $("mx-sprev").addEventListener("click", () => MV() && MV().skip(-1));
  $("mx-snext").addEventListener("click", () => MV() && MV().skip(1));
  $("mx-mprev").addEventListener("click", () => MV() && MV().select(MV().index() - 1));
  $("mx-mnext").addEventListener("click", () => MV() && MV().select(MV().index() + 1));
  const seek = $("mx-seek");
  seek.addEventListener("input", () => { seeking = true; });
  seek.addEventListener("change", () => { seeking = false; if (MV()) MV().seek(Number(seek.value) / 1000); });
}

// ------------------------------------------------------------------ frame loop

let raf = 0;
let last = 0;

function frame(t) {
  raf = requestAnimationFrame(frame);
  const dt = Math.min((t - last) / 1000, 0.1);
  last = t;
  const live = window.TechnoLive;
  const MV = window.TechnoMixVisual;
  // Whatever plays decides the source; when nothing plays the choice stays.
  const mixPlaying = !!(MV && MV.isPlaying());
  const livePlaying = !!(live && live.visual && live.visual().playing);
  if (mixPlaying && source !== "mix") source = "mix";
  else if (livePlaying && source !== "live") source = "live";
  if (source === "mix" && (!MV || !MV.count())) source = "live";
  const isMix = source === "mix";
  const v = isMix ? MV.visual() : live && live.visual ? live.visual() : null;
  const state = isMix
    ? (mixPlaying ? "playing" : "idle")
    : $("lv-play") ? $("lv-play").dataset.state : "idle";
  const playing = !!(v && v.playing);

  for (let i = 0; i < 2; i++) {
    const d = v && v.decks ? v.decks[i] : null;
    const has = !!(d && d.seed !== null && d.seed !== undefined);
    const audible = playing && has && d.gain > 0.04;
    const st = decks[i];
    // Record: eases to speed and back to a stop (a platter needs a moment).
    const target = audible ? SPEED : 0;
    st.vel += (target - st.vel) * Math.min(1, dt * (audible ? 1.6 : 2.2));
    if (Math.abs(st.vel) < 0.6) st.vel = 0;
    st.angle = (st.angle + st.vel * dt) % 360;
    const rec = $(`rec-${i}`);
    rec.setAttribute("transform", `rotate(${f(st.angle)} 110 110)`);
    $(`arm-${i}`).classList.toggle("down", audible || (playing && has));
    // Label text only when the song on the deck changes.
    const key = has ? `${d.seed}|${d.title}|${d.artist}` : "";
    if (key !== st.key) {
      st.key = key;
      // Every song gets its own label (layout, colours, type, decoration) from its seed.
      $(`lab-${i}`).innerHTML = has
        ? labelSvg(i, d.seed, d.artist || "", d.title || `Song ${d.seed}`)
        : '<circle cx="110" cy="110" r="58" class="tt-label"/>';
      $(`cap-${i}`).innerHTML = has
        ? `<b>${esc(d.title || "Song " + d.seed)}</b><span>${esc(d.artist || "")}</span>`
        : `<span>${playing ? "" : "Press play"}</span>`;
    }
    // Mixer channel.
    const g = has ? Math.max(0, Math.min(1, d.gain)) : 0;
    $(`knob-${i}`).style.bottom = `calc(${(g * 100).toFixed(1)}% - 7px)`;
    $(`fill-${i}`).style.height = `${(g * 100).toFixed(1)}%`;
    $(`lows-${i}`).classList.toggle("off", has && d.lows < 0.5);
    $(`lows-${i}`).classList.toggle("on", has && d.lows >= 0.5);
    $(`mx-new`).disabled = state === "loading";
  }
  // Centre: tempo, key, section, energy LEDs, play button, next transition.
  $("mx-bpm").textContent = v && v.bpm ? String(Math.round(v.bpm)) : "---";
  $("mx-key").textContent = v && v.camelot ? v.camelot : "--";
  $("mx-sec").textContent = v && v.section ? v.section : " ";
  const leds = $("mx-leds").children;
  const lit = v ? Math.round((v.energy || 0) * LEDS) : 0;
  for (let i = 0; i < LEDS; i++) leds[i].classList.toggle("on", i < lit);
  $("mx-play").dataset.state = state;
  $("mx-play").disabled = state === "loading";
  $("mx-play").setAttribute("aria-label", state === "playing" ? "Pause" : "Play");
  $("mx-next").textContent = v && v.next ? v.next : " ";
  // Source switch and the controls that only a mix has.
  const hasMixes = !!(MV && MV.count());
  $("src-live").setAttribute("aria-pressed", String(!isMix));
  $("src-mix").setAttribute("aria-pressed", String(isMix));
  $("src-mix").disabled = !hasMixes;
  $("src-mix").title = hasMixes ? "" : "No mixes for this genre yet";
  for (const id of ["mx-sprev", "mx-snext", "mx-seekrow", "mx-mixbar"]) $(id).hidden = !isMix;
  $("mx-new").hidden = isMix;
  if (isMix && v) {
    $("mx-mname").textContent = `${v.title} · ${v.mixIndex + 1}/${v.mixCount}`;
    $("mx-t0").textContent = fmt(v.t);
    $("mx-t1").textContent = fmt(v.duration);
    if (!seeking) $("mx-seek").value = v.duration ? String(Math.round((v.t / v.duration) * 1000)) : "0";
  }
  const fav = $("mx-fav");
  const isFav = isMix
    ? !!(MV && MV.isFavorite())
    : !!(live && live.isFavorite && live.isFavorite());
  fav.classList.toggle("on", isFav);
  fav.setAttribute("aria-pressed", String(isFav));
  fav.hidden = !playing;
  document.querySelectorAll(".rate").forEach((b) => { b.hidden = !playing; });
}

// ------------------------------------------------------------------ switch

function set(on, persist = true) {
  document.documentElement.dataset.visuals = on ? "1" : "0";
  const b = $("vis-btn");
  if (b) {
    b.setAttribute("aria-pressed", String(on));
    b.title = on ? "Leave visuals mode" : "Visuals mode";
  }
  if (persist) { try { localStorage.setItem(KEY, on ? "1" : "0"); } catch (_) {} }
  const url = new URL(location.href);
  if (on) url.searchParams.set("visuals", "1");
  else url.searchParams.delete("visuals");
  history.replaceState(null, "", url);
  $("visuals").hidden = !on;
  cancelAnimationFrame(raf);
  if (on) {
    build();
    last = performance.now();
    raf = requestAnimationFrame(frame);
    $("mx-vol").value = $("lv-vol").value;
  }
}

function init() {
  const btn = document.createElement("button");
  btn.id = "vis-btn";
  btn.className = "vis-btn";
  btn.type = "button";
  btn.setAttribute("aria-label", "Visuals mode");
  btn.setAttribute("aria-pressed", "false");
  btn.innerHTML = '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3.2"/><path d="M12 3a9 9 0 0 1 7.8 4.5" stroke-width="2.4"/></svg>';
  btn.addEventListener("click", () => set(document.documentElement.dataset.visuals !== "1"));
  document.querySelector("main").prepend(btn);
  const q = new URLSearchParams(location.search).get("visuals");
  set(q === "1" || (q === null && read()), false);
  // A hidden tab does not need animation frames.
  document.addEventListener("visibilitychange", () => {
    if (document.documentElement.dataset.visuals !== "1") return;
    cancelAnimationFrame(raf);
    if (document.visibilityState === "visible") {
      last = performance.now();
      raf = requestAnimationFrame(frame);
    }
  });
}

init();
window.TechnoVisuals = { set };
