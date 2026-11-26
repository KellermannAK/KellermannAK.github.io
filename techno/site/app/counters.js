// Public counters under the genre switcher: monthly listeners and monthly listening hours
// for the current genre and in total (last 30 days, from the public_stats() database
// function). Hidden if the database is not configured or the function is not installed yet.
const cfg = window.TECHNO_CONFIG || {};
const box = document.getElementById("g-counters");
let data = null;

const nf = new Intl.NumberFormat("en-US");
const num = (n) => nf.format(Math.round(n));
const hours = (h) => (h >= 100 ? nf.format(Math.round(h)) : nf.format(Math.round(h * 10) / 10));
const line = (label, s) =>
  `<span><b>${label}</b> ${num(s.listeners)} monthly listener${Math.round(s.listeners) === 1 ? "" : "s"} · ${hours(s.hours)} listening hour${s.hours === 1 ? "" : "s"}</span>`;

function render() {
  if (!box) return;
  if (!data) {
    box.hidden = true;
    return;
  }
  const g = window.TechnoGenre ? window.TechnoGenre.current() : { id: "techno", name: "Techno" };
  const zero = { listeners: 0, hours: 0 };
  const mine = (data.genres && data.genres[g.id]) || zero;
  box.innerHTML = line(g.name, mine) + line("All genres", data.total || zero);
  box.hidden = false;
}

async function load() {
  if (!cfg.supabaseUrl || !cfg.supabaseAnonKey) return;
  try {
    const r = await fetch(`${cfg.supabaseUrl}/rest/v1/rpc/public_stats`, {
      method: "POST",
      headers: {
        apikey: cfg.supabaseAnonKey,
        Authorization: `Bearer ${cfg.supabaseAnonKey}`,
        "Content-Type": "application/json",
      },
      body: "{}",
    });
    if (!r.ok) return;
    data = await r.json();
    render();
  } catch (_) { /* counters are optional */ }
}

window.addEventListener("genrechange", render);
load();
setInterval(load, 10 * 60 * 1000);
