// Account card: shows the session state, registration by e-mail link, display name.
import * as DB from "./db.js";
import * as Stats from "./stats.js";

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const dur = (s) => {
  if (s >= 3600) return `${Math.floor(s / 3600)} h ${Math.round((s % 3600) / 60)} min`;
  if (s >= 60) return `${Math.round(s / 60)} min`;
  return `${s} s`;
};

async function render() {
  const box = $("account");
  if (!DB.enabled || !box) return;
  $("account-title").hidden = false;
  box.hidden = false;
  box.innerHTML = '<p class="meta">Verbinde…</p>';
  const a = await DB.account();
  if (!a) {
    box.innerHTML = '<p class="meta">Konto-Funktionen gerade nicht erreichbar. Feedback wird lokal gesammelt.</p>';
    return;
  }
  const status = a.anonymous
    ? (a.pendingEmail ? `Bestätigungs-Link an <b>${esc(a.pendingEmail)}</b> geschickt – bitte in der E-Mail klicken.` : "Du hörst anonym. Dein Feedback wird trotzdem gespeichert.")
    : `Angemeldet als <b>${esc(a.displayName || a.email || "Nutzer")}</b>${a.displayName && a.email ? ` (${esc(a.email)})` : ""}.`;
  box.innerHTML = `
    <p style="margin:0 0 6px">${status}</p>
    <p class="meta" style="margin:0 0 12px">${a.feedbackCount} Feedback${a.feedbackCount === 1 ? "" : "s"} gespeichert${a.stats && a.stats.visits ? ` · ${a.stats.visits} Besuch${a.stats.visits === 1 ? "" : "e"} · ${dur(a.stats.listen_seconds)} gehört · ${a.stats.songs} Song${a.stats.songs === 1 ? "" : "s"}` : ""}.</p>
    ${a.anonymous ? `
      <div class="seed-row"><input id="acc-email" type="email" inputmode="email" autocomplete="email" placeholder="E-Mail" aria-label="E-Mail"><button class="btn" id="acc-register">Registrieren</button></div>
      <p class="label" style="margin:6px 0 0">Kein Passwort: du bekommst einen Link per E-Mail. Schon registriert? <button class="linkbtn" id="acc-signin">Auf diesem Gerät anmelden</button></p>`
    : `
      <div class="seed-row"><input id="acc-name" maxlength="40" placeholder="Anzeigename" aria-label="Anzeigename" value="${esc(a.displayName || "")}"><button class="btn" id="acc-save">Speichern</button></div>
      <p class="label" style="margin:6px 0 0"><button class="linkbtn" id="acc-signout">Abmelden</button></p>`}
    <p class="label" style="margin:12px 0 0">Nutzungsstatistik: <b>${Stats.enabled() ? "an" : "aus"}</b>
      – Seitenaufrufe, Zeit auf der Seite und Hörzeit pro Song (auf 10 s genau), ohne IP-Adresse.
      <button class="linkbtn" id="acc-stats">${Stats.enabled() ? "Abschalten" : "Einschalten"}</button></p>
    <p class="label" id="acc-msg" style="margin:8px 0 0"></p>`;
  $("acc-stats").addEventListener("click", () => {
    Stats.setMode(Stats.enabled() ? "off" : "on");
    render();
  });
  const msg = (t) => { $("acc-msg").textContent = t; };
  const email = () => ($("acc-email").value || "").trim();
  if (a.anonymous) {
    $("acc-register").addEventListener("click", async () => {
      if (!/^\S+@\S+\.\S+$/.test(email())) return msg("Bitte eine gültige E-Mail eingeben.");
      const r = await DB.register(email());
      msg(r.error ? `Fehler: ${r.error}` : "Link verschickt – bitte E-Mail öffnen und bestätigen.");
    });
    $("acc-signin").addEventListener("click", async () => {
      if (!/^\S+@\S+\.\S+$/.test(email())) return msg("Bitte deine E-Mail oben eingeben.");
      const r = await DB.signIn(email());
      msg(r.error ? `Fehler: ${r.error}` : "Anmelde-Link verschickt.");
    });
  } else {
    $("acc-save").addEventListener("click", async () => {
      const r = await DB.setDisplayName($("acc-name").value);
      msg(r.error ? `Fehler: ${r.error}` : "Gespeichert.");
      if (!r.error) render();
    });
    $("acc-signout").addEventListener("click", async () => { await DB.signOut(); render(); });
  }
}

window.TechnoAccount = { render };
render();
