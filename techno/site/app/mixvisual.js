// Mixes for the visuals mode: turns the playing position of a mix (an MP3 with a tracklist)
// into what the turntables and the mixer show. Song i sits on deck i % 2, as in the engine;
// during a blend the faders follow the same curves as the live set's DJ strips. The blend
// timing comes from the tracklist (blend_bars / in_bar / swap_after); mixes rendered before
// those fields existed use estimates per transition type.
(function () {
  const SECTIONS = { intro: "Intro", groove: "Groove", build: "Build-up", breakdown: "Breakdown", drop: "Drop", outro: "Outro" };
  const ENERGY = { intro: 0.35, groove: 0.65, build: 0.85, breakdown: 0.35, drop: 1, outro: 0.3 };
  const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
  // [blend bars, in-bar, swap after] for mixes without timing data.
  const ESTIMATE = { "cut": [1, 32, 0], "fast blend": [12, 20, 6], "slow blend": [48, 0, 24], "filter blend": [24, 8, 12] };
  const EMPTY = { seed: null, title: "", artist: "", gain: 0, lows: 0, hp: 0 };

  let list = [];
  let sel = 0;
  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const mmss = (t) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;

  function blendOf(s) {
    let bars = s.blend_bars;
    let inBar = s.in_bar;
    let swap = s.swap_after;
    if (bars === null || bars === undefined) {
      [bars, inBar, swap] = ESTIMATE[s.transition] || [16, 16, 8];
    }
    return { bars, inBar, swap, bar: 240 / s.bpm };
  }

  /** Sections of a song from its structure text ("bar  33  1:02  groove  32 bars  ..."). */
  function sectionsOf(s) {
    if (!s._secs) {
      s._secs = [];
      for (const m of String(s.structure || "").matchAll(/bar\s+(\d+)\s+\d+:\d+\s+(\w+)\s+(\d+) bars/g)) {
        s._secs.push({ from: Number(m[1]) - 1, kind: m[2], bars: Number(m[3]) });
      }
    }
    return s._secs;
  }

  function songIndexAt(songs, t) {
    let cur = 0;
    songs.forEach((s, i) => { if (t >= s.start) cur = i; });
    return cur;
  }

  function compute(m) {
    const songs = m.songs;
    const t = m.audio.currentTime || 0;
    const cur = songIndexAt(songs, t);
    const deck = (i, gain, lows, hp) => ({
      seed: songs[i].seed, title: songs[i].title || "", artist: songs[i].artist || "", gain, lows, hp,
    });
    const decks = [EMPTY, EMPTY];
    let main = cur;
    const s = songs[cur];
    const b = cur > 0 ? blendOf(s) : null;
    if (b && t < s.start + b.bars * b.bar) {
      // Blend from the previous song into this one.
      const x = (t - s.start) / b.bar;
      const half = Math.max(b.bars / 2, 1);
      const fadeIn = clamp(x / half);
      const fadeOut = clamp(1 - (x - half) / half);
      const swapped = x >= b.swap;
      let gIn = 1;
      let gOut = 0;
      let hpOut = 0;
      if (s.transition === "fast blend" || s.transition === "slow blend") {
        gIn = Math.sin(fadeIn * Math.PI / 2);
        gOut = Math.sin(fadeOut * Math.PI / 2);
      } else if (s.transition === "filter blend") {
        gIn = Math.min(fadeIn * 3, 1);
        gOut = Math.min(fadeOut * 2, 1);
        hpOut = clamp(x / b.bars) * 0.95;
      }
      decks[cur % 2] = deck(cur, gIn, swapped ? 1 : 0, 0);
      decks[(cur - 1) % 2] = deck(cur - 1, gOut, swapped ? 0 : 1, hpOut);
      main = swapped ? cur : cur - 1;
    } else {
      decks[cur % 2] = deck(cur, 1, 1, 0);
    }
    // Position inside the main song, for its section and energy.
    const ms = songs[main];
    const mb = main > 0 ? blendOf(ms) : { inBar: 0 };
    const bars = mb.inBar + (t - ms.start) / (240 / ms.bpm);
    const sec = sectionsOf(ms).find((x) => bars >= x.from && bars < x.from + x.bars);
    const next = songs[cur + 1];
    return {
      playing: !m.audio.paused && !m.audio.ended,
      bpm: ms.bpm,
      camelot: ms.camelot,
      section: sec ? SECTIONS[sec.kind] || "" : "",
      energy: sec ? ENERGY[sec.kind] || 0.5 : 0,
      next: next ? `Next: ${next.title || "song " + (cur + 2)} in ${mmss(Math.max(0, next.start - t))}` : "Last song of the mix",
      slot: main % 2,
      decks,
      song: main,
      t,
      duration: m.set.duration || m.audio.duration || 0,
    };
  }

  const playingIndex = () => list.findIndex((m) => !m.audio.paused && !m.audio.ended);

  function seekTo(m, t) {
    const go = () => { m.audio.currentTime = clamp(t, 0, (m.audio.duration || m.set.duration || t)); };
    if (m.audio.readyState >= 1) go();
    else m.audio.addEventListener("loadedmetadata", go, { once: true });
  }

  const api = {
    /** The mixes of the current genre (newest first), each { audio, songs, set, seed, genre, build }. */
    set(l) {
      list = l;
      sel = 0;
      window.dispatchEvent(new CustomEvent("mixlist"));
    },
    count: () => list.length,
    index: () => sel,
    current: () => list[sel] || null,
    playingIndex,
    isPlaying: () => playingIndex() >= 0,
    /** Choose another mix; keeps playing if one was playing. */
    select(i) {
      if (!list.length) return;
      const was = playingIndex() >= 0;
      i = ((i % list.length) + list.length) % list.length;
      if (i === sel) return;
      list.forEach((m) => m.audio.pause());
      sel = i;
      if (was) list[sel].audio.play().catch(() => {});
    },
    toggle() {
      const m = list[playingIndex() >= 0 ? playingIndex() : sel];
      if (!m) return;
      if (m.audio.paused) m.audio.play().catch(() => {});
      else m.audio.pause();
    },
    /** Jump to the next (+1) or previous (-1) song of the playing / selected mix. */
    skip(delta) {
      const m = list[playingIndex() >= 0 ? playingIndex() : sel];
      if (!m) return;
      const t = m.audio.currentTime || 0;
      const cur = songIndexAt(m.songs, t);
      // "Previous" first returns to the start of the current song.
      const target = delta < 0 && t - m.songs[cur].start > 5 ? cur : cur + delta;
      const i = clamp(target, 0, m.songs.length - 1);
      seekTo(m, m.songs[i].start + (i > 0 ? 1 : 0));
    },
    seek(fraction) {
      const m = list[playingIndex() >= 0 ? playingIndex() : sel];
      if (m) seekTo(m, fraction * (m.set.duration || m.audio.duration || 0));
    },
    setVolume(v) { list.forEach((m) => { m.audio.volume = clamp(v); }); },
    /** Data for the turntables and the mixer (same shape as the live set's). */
    visual() {
      const m = list[playingIndex() >= 0 ? playingIndex() : sel];
      if (!m) return null;
      const v = compute(m);
      v.title = `Mix #${m.seed}`;
      v.mixIndex = list.indexOf(m);
      v.mixCount = list.length;
      return v;
    },
    /** Favourite heart for the song that is playing (or about to). */
    info() {
      const m = list[playingIndex() >= 0 ? playingIndex() : sel];
      if (!m) return null;
      const s = m.songs[songIndexAt(m.songs, m.audio.currentTime || 0)];
      const root = NOTE_NAMES.indexOf(String(s.key).split(" ")[0]);
      if (root < 0) return null;
      return { genre: m.genre, seed: s.seed, root, bpm: s.bpm, title: s.title || "", artist: s.artist || "", key: s.key, build: m.build };
    },
    isFavorite() {
      const i = api.info();
      return !!(i && window.TechnoFavs && window.TechnoFavs.has(i.genre, i.seed));
    },
    toggleFavorite() {
      const i = api.info();
      if (i && window.TechnoFavs) window.TechnoFavs.toggle(i);
    },
  };
  window.TechnoMixVisual = api;
})();
