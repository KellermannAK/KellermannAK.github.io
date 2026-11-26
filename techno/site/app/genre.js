// Genre switcher: arrows left / right turn the whole page like a cube; every genre has
// its own colour theme. The current genre is kept in the URL (?genre=house) and on this
// device, and announced to the other modules with a "genrechange" event on window.
(function () {
  const GENRES = [
    { id: "techno", name: "Techno", index: 0, profile: "hard_techno_simple", theme: "#0d0d0f" },
    { id: "house", name: "House", index: 1, profile: "house", theme: "#170d24" },
  ];
  const KEY = "techno-genre";
  const byId = (id) => GENRES.find((g) => g.id === id);

  function initial() {
    const q = new URLSearchParams(location.search).get("genre");
    if (q && byId(q)) return q;
    try {
      const s = localStorage.getItem(KEY);
      if (s && byId(s)) return s;
    } catch (_) {}
    return "techno";
  }

  let current = initial();
  let busy = false;

  function apply() {
    const g = byId(current);
    document.documentElement.dataset.genre = g.id;
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", g.theme);
    const name = document.getElementById("g-name");
    if (name) name.textContent = g.name;
    const i = GENRES.indexOf(g);
    const prev = GENRES[(i + GENRES.length - 1) % GENRES.length];
    const next = GENRES[(i + 1) % GENRES.length];
    const p = document.getElementById("g-prev");
    const n = document.getElementById("g-next");
    if (p) p.setAttribute("aria-label", `Zu ${prev.name}`);
    if (n) n.setAttribute("aria-label", `Zu ${next.name}`);
    const url = new URL(location.href);
    if (g.id === "techno") url.searchParams.delete("genre");
    else url.searchParams.set("genre", g.id);
    history.replaceState(null, "", url);
    try { localStorage.setItem(KEY, g.id); } catch (_) {}
  }

  function setGenre(id) {
    if (id === current || !byId(id)) return;
    current = id;
    apply();
    window.dispatchEvent(new CustomEvent("genrechange", { detail: byId(id) }));
  }

  const reduced = () => window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;

  /** Rotate the page out (around a point behind it, like a cube face), swap, rotate in. */
  async function turn(dir) {
    if (busy) return;
    const i = GENRES.findIndex((g) => g.id === current);
    const target = GENRES[(i + dir + GENRES.length) % GENRES.length].id;
    const face = document.getElementById("face");
    const stage = document.getElementById("stage");
    if (!face || !stage || reduced() || !face.animate) {
      setGenre(target);
      return;
    }
    busy = true;
    const w = face.offsetWidth || innerWidth;
    // Turn around the middle of the visible part of the page, not of the whole (long) page.
    const top = face.getBoundingClientRect().top + scrollY;
    const midY = scrollY + innerHeight / 2 - top;
    stage.style.perspective = `${Math.max(900, w * 2)}px`;
    stage.style.perspectiveOrigin = `50% ${innerHeight / 2 - stage.getBoundingClientRect().top}px`;
    face.style.transformOrigin = `50% ${midY}px ${-w / 2}px`;
    face.style.backfaceVisibility = "hidden";
    const out = dir > 0 ? -90 : 90;
    try {
      await face.animate(
        [{ transform: "rotateY(0deg)", opacity: 1 }, { transform: `rotateY(${out}deg)`, opacity: 0.55 }],
        { duration: 300, easing: "cubic-bezier(.55,0,.8,.4)", fill: "forwards" },
      ).finished;
      setGenre(target);
      await face.animate(
        [{ transform: `rotateY(${-out}deg)`, opacity: 0.55 }, { transform: "rotateY(0deg)", opacity: 1 }],
        { duration: 360, easing: "cubic-bezier(.2,.6,.35,1)", fill: "forwards" },
      ).finished;
    } finally {
      face.getAnimations().forEach((a) => a.cancel());
      face.style.transformOrigin = "";
      stage.style.perspective = "";
      stage.style.perspectiveOrigin = "";
      busy = false;
    }
  }

  function init() {
    apply();
    const p = document.getElementById("g-prev");
    const n = document.getElementById("g-next");
    if (p) p.addEventListener("click", () => turn(-1));
    if (n) n.addEventListener("click", () => turn(1));
    // Keyboard: arrow keys when nothing editable or adjustable has the focus.
    document.addEventListener("keydown", (e) => {
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      const t = e.target;
      if (t && (t.closest("input, textarea, select, audio, dialog, [contenteditable]"))) return;
      if (e.key === "ArrowLeft") turn(-1);
      if (e.key === "ArrowRight") turn(1);
    });
    // Horizontal swipe on the header.
    const head = document.getElementById("g-head");
    if (head) {
      let x0 = null;
      let y0 = null;
      head.addEventListener("touchstart", (e) => { x0 = e.touches[0].clientX; y0 = e.touches[0].clientY; }, { passive: true });
      head.addEventListener("touchend", (e) => {
        if (x0 === null) return;
        const dx = e.changedTouches[0].clientX - x0;
        const dy = e.changedTouches[0].clientY - y0;
        x0 = null;
        if (Math.abs(dx) > 50 && Math.abs(dx) > 2 * Math.abs(dy)) turn(dx < 0 ? 1 : -1);
      }, { passive: true });
    }
  }

  // Apply the theme before the first paint (this script is loaded in <head>).
  document.documentElement.dataset.genre = current;
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();

  window.TechnoGenre = {
    list: GENRES,
    current: () => byId(current),
    set: setGenre,
    turn,
  };
})();
