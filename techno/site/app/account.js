// Account: a small circle in the top-right corner (person icon, check mark when logged in).
// Clicking it opens a panel to register or sign in by e-mail link, set a display name, sign
// out and switch the usage statistics off. Nothing else about the account is on the page.
import * as DB from "./db.js";
import * as Stats from "./stats.js";

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const PERSON = '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="8.5" r="3.6"/><path d="M4.6 20c.9-4 4-5.6 7.4-5.6s6.5 1.6 7.4 5.6"/></svg>';
const CHECK = '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 12.8l4.2 4.2L18.5 8"/></svg>';

let btn;
let panel;
let state = null;

function build() {
  const main = document.querySelector("main");
  btn = document.createElement("button");
  btn.className = "acc-btn";
  btn.type = "button";
  btn.setAttribute("aria-haspopup", "dialog");
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-label", "Konto");
  btn.innerHTML = PERSON;
  panel = document.createElement("div");
  panel.className = "acc-panel";
  panel.hidden = true;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Konto");
  main.prepend(panel);
  main.prepend(btn);

  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (panel.hidden) open();
    else close();
  });
  document.addEventListener("click", (e) => {
    if (!panel.hidden && !panel.contains(e.target) && !btn.contains(e.target)) close();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !panel.hidden) {
      close();
      btn.focus();
    }
  });
}

function open() {
  panel.hidden = false;
  btn.setAttribute("aria-expanded", "true");
  renderPanel();
  refresh();
}

function close() {
  panel.hidden = true;
  btn.setAttribute("aria-expanded", "false");
}

/** Update the circle (and the panel if it is open) from the current session. */
async function refresh() {
  const a = await DB.account();
  state = a;
  const loggedIn = !!(a && !a.anonymous);
  btn.classList.toggle("in", loggedIn);
  btn.innerHTML = loggedIn ? CHECK : PERSON;
  btn.title = loggedIn ? `Angemeldet als ${a.displayName || a.email || "Nutzer"}` : "Konto";
  btn.setAttribute("aria-label", loggedIn ? "Konto, angemeldet" : "Konto, nicht angemeldet");
  if (!panel.hidden) renderPanel();
}

let timer = 0;
const refreshSoon = () => {
  clearTimeout(timer);
  timer = setTimeout(refresh, 300);
};

function renderPanel() {
  const a = state;
  const msgOld = panel.querySelector("#acc-msg");
  const keepMsg = msgOld ? msgOld.textContent : "";
  const stats = `
    <label class="acc-opt"><input type="checkbox" id="acc-stats" ${Stats.enabled() ? "checked" : ""}>
      <span>Nutzungsstatistik senden <small>(anonym, ohne IP-Adresse – siehe Datenschutz unten)</small></span></label>`;
  if (!a) {
    panel.innerHTML = `<p class="acc-line">Verbinde …</p>${stats}`;
  } else if (a.anonymous) {
    const status = a.pendingEmail
      ? `Bestätigungs-Link an <b>${esc(a.pendingEmail)}</b> geschickt – bitte in der E-Mail klicken.`
      : "Du bist nicht angemeldet. Dein Feedback wird trotzdem gespeichert.";
    panel.innerHTML = `
      <p class="acc-line">${status}</p>
      <div class="seed-row"><input id="acc-email" type="email" inputmode="email" autocomplete="email" placeholder="E-Mail" aria-label="E-Mail"><button class="btn" id="acc-register">Registrieren</button></div>
      <p class="label" style="margin:6px 0 0">Kein Passwort: du bekommst einen Link per E-Mail. Schon registriert? <button class="linkbtn" id="acc-signin">Anmelden</button></p>
      ${stats}<p class="label" id="acc-msg"></p>`;
  } else {
    panel.innerHTML = `
      <p class="acc-line">Angemeldet als <b>${esc(a.displayName || a.email || "Nutzer")}</b>${a.displayName && a.email ? `<br><small>${esc(a.email)}</small>` : ""}</p>
      <div class="seed-row"><input id="acc-name" maxlength="40" placeholder="Anzeigename" aria-label="Anzeigename" value="${esc(a.displayName || "")}"><button class="btn" id="acc-save">Speichern</button></div>
      <p class="label" style="margin:6px 0 0"><button class="linkbtn" id="acc-signout">Abmelden</button></p>
      ${stats}<p class="label" id="acc-msg"></p>`;
  }
  const $ = (id) => panel.querySelector(`#${id}`);
  const msg = (t) => { const m = $("acc-msg"); if (m) m.textContent = t; };
  msg(keepMsg);
  const email = () => (($("acc-email") || {}).value || "").trim();
  const valid = () => /^\S+@\S+\.\S+$/.test(email());
  $("acc-stats").addEventListener("change", (e) => Stats.setMode(e.target.checked ? "on" : "off"));
  if (a && a.anonymous) {
    $("acc-register").addEventListener("click", async () => {
      if (!valid()) return msg("Bitte eine gültige E-Mail eingeben.");
      const r = await DB.register(email());
      msg(r.error ? `Fehler: ${r.error}` : "Link verschickt – bitte E-Mail öffnen und bestätigen.");
    });
    $("acc-signin").addEventListener("click", async () => {
      if (!valid()) return msg("Bitte deine E-Mail oben eingeben.");
      const r = await DB.signIn(email());
      msg(r.error ? `Fehler: ${r.error}` : "Anmelde-Link verschickt.");
    });
  } else if (a) {
    $("acc-save").addEventListener("click", async () => {
      const r = await DB.setDisplayName($("acc-name").value);
      if (r.error) return msg(`Fehler: ${r.error}`);
      await refresh();
      msg("Gespeichert.");
    });
    $("acc-signout").addEventListener("click", async () => {
      await DB.signOut();
      await refresh();
    });
  }
}

if (DB.enabled) {
  build();
  DB.onAuth(refreshSoon);
  refresh();
}
