// Visuals mode: hides everything below the genre switcher and shows two turntables with a
// mixer in between. The records spin while their deck is audible, the artist and the title
// run around the record label, and the mixer shows what the set engine does: channel faders,
// low-band kills, the playing song's BPM and key, the arrangement energy and the next
// transition. All data comes from the live set (`TechnoLive.visual()`); the buttons in the
// mixer press the real controls of the live card.
const KEY = "techno-visuals";
const SPEED = 200; // degrees per second: 33 1/3 rpm
const LEDS = 10;

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const read = () => {
  try { return localStorage.getItem(KEY) === "1"; } catch (_) { return false; }
};

// ------------------------------------------------------------------ turntable drawing

const pt = (cx, cy, r, deg) => {
  const a = (deg * Math.PI) / 180;
  return [cx + r * Math.sin(a), cy - r * Math.cos(a)];
};
const f = (n) => n.toFixed(2);

function deckSvg(i) {
  const [c, R] = [110, 98];
  const rt = 43; // artist (top arc)
  const rb = 46; // title (bottom arc)
  const span = 85;
  const [tx0, ty0] = pt(c, c, rt, -span);
  const [tx1, ty1] = pt(c, c, rt, span);
  const [bx0, by0] = pt(c, c, rb, 180 + span);
  const [bx1, by1] = pt(c, c, rb, 180 - span);
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
      <circle cx="${c}" cy="${c}" r="58" class="tt-label"/>
      <path id="tp-${i}" d="M${f(tx0)} ${f(ty0)} A${rt} ${rt} 0 0 1 ${f(tx1)} ${f(ty1)}" fill="none"/>
      <path id="bp-${i}" d="M${f(bx0)} ${f(by0)} A${rb} ${rb} 0 0 0 ${f(bx1)} ${f(by1)}" fill="none"/>
      <text class="tt-artist"><textPath href="#tp-${i}" startOffset="50%" text-anchor="middle" id="ta-${i}"></textPath></text>
      <text class="tt-title"><textPath href="#bp-${i}" startOffset="50%" text-anchor="middle" id="tt-${i}"></textPath></text>
      <text class="tt-genre" x="${c}" y="${c - 4}" text-anchor="middle" id="tg-${i}"></text>
      <text class="tt-bpm" x="${c}" y="${c + 9}" text-anchor="middle" id="tb-${i}"></text>
      <circle cx="${c}" cy="${c}" r="3.2" fill="#e6e6e6"/>
    </g>
    <g class="arm" id="arm-${i}">
      <circle cx="196" cy="30" r="10" class="tt-pivot"/>
      <line x1="196" y1="30" x2="170" y2="148" class="tt-tube"/>
      <rect x="160" y="144" width="15" height="24" rx="3" transform="rotate(14 170 148)" class="tt-head"/>
    </g>
  </svg>`;
}

function fit(el, text, arc, size) {
  // Shrink the text to the arc: glyphs and spacing are compressed when it is too long.
  const t = text.length > 34 ? `${text.slice(0, 33)}…` : text;
  el.textContent = t;
  const natural = t.length * size * 0.56;
  if (natural > arc) {
    el.setAttribute("textLength", String(arc));
    el.setAttribute("lengthAdjust", "spacingAndGlyphs");
  } else {
    el.removeAttribute("textLength");
    el.removeAttribute("lengthAdjust");
  }
}

// ------------------------------------------------------------------ build

let built = false;
const decks = [{ angle: 0, vel: 0, key: "" }, { angle: 0, vel: 0, key: "" }];

function build() {
  if (built) return;
  built = true;
  const box = $("visuals");
  box.innerHTML = `
    <div class="vis">
      <div class="vis-deck" data-i="0">${deckSvg(0)}<div class="vis-cap" id="cap-0"></div></div>
      <div class="vis-mixer" aria-label="Mixer">
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
            <button class="mx-play" id="mx-play" aria-label="Play">
              <svg class="ico-play" viewBox="0 0 24 24"><path d="M7 4.5v15l13-7.5z"/></svg>
              <svg class="ico-pause" viewBox="0 0 24 24"><path d="M6 4h4.5v16H6zM13.5 4H18v16h-4.5z"/></svg>
            </button>
            <input type="range" id="mx-vol" min="0" max="1" step="0.01" aria-label="Volume">
            <div class="mx-row">
              <button class="heart" id="mx-fav" aria-label="Add to favorites" aria-pressed="false"><svg viewBox="0 0 24 24"><path d="M12 20.4C5.2 15.2 3 12.1 3 8.9 3 6.2 5 4.5 7.3 4.5c1.9 0 3.5 1.1 4.7 3 1.2-1.9 2.8-3 4.7-3C19 4.5 21 6.2 21 8.9c0 3.2-2.2 6.3-9 11.5z"/></svg></button>
              <button class="btn mx-new" id="mx-new">New set</button>
            </div>
          </div>
          <div class="mx-ch" data-i="1">
            <div class="mx-lows" id="lows-1">LOW</div>
            <div class="mx-fader"><div class="mx-fill" id="fill-1"></div><div class="mx-knob" id="knob-1"></div></div>
            <div class="mx-name">B</div>
          </div>
        </div>
        <div class="mx-next" id="mx-next">&nbsp;</div>
      </div>
      <div class="vis-deck" data-i="1">${deckSvg(1)}<div class="vis-cap" id="cap-1"></div></div>
    </div>`;
  const vol = $("mx-vol");
  vol.value = $("lv-vol").value;
  vol.addEventListener("input", () => {
    $("lv-vol").value = vol.value;
    $("lv-vol").dispatchEvent(new Event("input", { bubbles: true }));
  });
  $("mx-play").addEventListener("click", () => $("lv-play").click());
  $("mx-new").addEventListener("click", () => $("lv-new").click());
  $("mx-fav").addEventListener("click", () => window.TechnoLive && window.TechnoLive.toggleFavorite());
}

// ------------------------------------------------------------------ frame loop

let raf = 0;
let last = 0;

function frame(t) {
  raf = requestAnimationFrame(frame);
  const dt = Math.min((t - last) / 1000, 0.1);
  last = t;
  const live = window.TechnoLive;
  const v = live && live.visual ? live.visual() : null;
  const state = $("lv-play") ? $("lv-play").dataset.state : "idle";
  const playing = !!(v && v.playing);

  const gname = (window.TechnoGenre ? window.TechnoGenre.current().name : "").toUpperCase();
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
      fit($(`ta-${i}`), has ? d.artist || "" : "", 2 * Math.PI * 43 * (170 / 360), 9.5);
      fit($(`tt-${i}`), has ? d.title || `Song ${d.seed}` : "", 2 * Math.PI * 46 * (170 / 360), 9.5);
      $(`tg-${i}`).textContent = gname;
      $(`tb-${i}`).textContent = has && v.bpm ? `${Math.round(v.bpm)} BPM` : "";
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
  const fav = $("mx-fav");
  const isFav = !!(live && live.isFavorite && live.isFavorite());
  fav.classList.toggle("on", isFav);
  fav.setAttribute("aria-pressed", String(isFav));
  fav.hidden = !playing;
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
