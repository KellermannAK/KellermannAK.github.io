// Listening feedback: thumbs up / down plus an optional comment.
// Tapping a thumb captures the moment; "Speichern" stores it on this device right away
// (no page change, nothing can be blocked). The collected list can later be copied,
// shared (e.g. into the Claude chat) or sent as ONE pre-filled GitHub issue via a plain link.
(function () {
  const OWNER = "KellermannAK";
  const REPO = "KellermannAK.github.io";
  const KEY = "techno-feedback-v2";
  const MAX_URL = 7500;

  let build = { commit: "", version: { engine: "?", profile: "?" } };
  fetch("songs.json", { cache: "no-store" })
    .then((r) => r.json())
    .then((j) => { build = { commit: j.commit || "", version: j.version || build.version }; })
    .catch(() => {});

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

  function load() {
    try { return JSON.parse(localStorage.getItem(KEY) || "[]"); } catch (_) { return []; }
  }
  function save(list) {
    try { localStorage.setItem(KEY, JSON.stringify(list)); } catch (_) { /* storage unavailable */ }
  }

  function where(ctx) {
    const parts = [];
    if (ctx.mixSeed !== undefined) parts.push(`Mix ${ctx.mixSeed} @ ${ctx.time}`);
    else parts.push(`Live-Set ${ctx.setSeed} @ ${ctx.time}`);
    if (ctx.songIndex !== undefined) parts.push(`Song ${ctx.songIndex + 1}${ctx.songSeed !== undefined ? ` (Seed ${ctx.songSeed})` : ""}`);
    if (ctx.section) parts.push(`${ctx.section}${ctx.sectionBar ? ` ${ctx.sectionBar}` : ""}`);
    if (ctx.songBar) parts.push(`Song-Takt ${ctx.songBar}`);
    if (ctx.key) parts.push(`${ctx.key}${ctx.camelot ? ` (${ctx.camelot})` : ""}`);
    return parts.join(" · ");
  }

  function toast(msg) {
    let t = document.getElementById("fb-toast");
    if (!t) {
      t = document.createElement("div");
      t.id = "fb-toast";
      document.body.appendChild(t);
    }
    t.textContent = msg;
    t.classList.add("show");
    clearTimeout(t._h);
    t._h = setTimeout(() => t.classList.remove("show"), 2200);
  }

  // ------------------------------------------------------------------ capture dialog

  function dialog() {
    let d = document.getElementById("fb-dialog");
    if (d) return d;
    d = document.createElement("dialog");
    d.id = "fb-dialog";
    d.innerHTML = `
      <div class="fb">
        <div class="fb-thumbs">
          <button type="button" class="fb-thumb" data-r="up" aria-label="Daumen hoch">👍</button>
          <button type="button" class="fb-thumb" data-r="down" aria-label="Daumen runter">👎</button>
        </div>
        <p class="fb-where"></p>
        <textarea rows="4" placeholder="Optional: Was genau? z.B. „klickendes Geräusch“, „Bass zu leise“, „super Drop“"></textarea>
        <div class="fb-actions">
          <button type="button" class="btn fb-cancel">Abbrechen</button>
          <button type="button" class="play fb-save">Speichern</button>
        </div>
      </div>`;
    document.body.appendChild(d);
    d.querySelectorAll(".fb-thumb").forEach((b) => b.addEventListener("click", () => setRating(d, b.dataset.r)));
    d.querySelector(".fb-cancel").addEventListener("click", () => d.close());
    d.querySelector(".fb-save").addEventListener("click", () => {
      const list = load();
      list.push({
        at: new Date().toISOString(),
        up: d.dataset.rating === "up",
        comment: d.querySelector("textarea").value.trim(),
        where: where(d.ctx),
        ctx: d.ctx,
        engine: build.version.engine,
        profile: build.version.profile,
        build: (build.commit || "").slice(0, 7),
      });
      save(list);
      d.close();
      renderInbox();
      toast(`Gespeichert – ${list.length} Feedback${list.length === 1 ? "" : "s"} gesammelt`);
    });
    return d;
  }

  function setRating(d, r) {
    d.dataset.rating = r;
    d.querySelectorAll(".fb-thumb").forEach((b) => b.classList.toggle("on", b.dataset.r === r));
  }

  function open(ctx, rating) {
    const d = dialog();
    d.ctx = ctx;
    setRating(d, rating);
    d.querySelector("textarea").value = "";
    d.querySelector(".fb-where").textContent = where(ctx);
    d.showModal();
  }

  // ------------------------------------------------------------------ inbox / export

  function exportText(list) {
    const v = list.length ? list[list.length - 1] : { engine: build.version.engine, profile: build.version.profile, build: "" };
    const lines = [`Techno-Feedback (${list.length}) – Engine ${v.engine} · Profil ${v.profile} · Build ${v.build || "?"}`, ""];
    list.forEach((e, i) => {
      lines.push(`${i + 1}. ${e.up ? "👍" : "👎"} ${e.where}${e.build ? ` · Build ${e.build}` : ""}`);
      if (e.comment) lines.push(`   „${e.comment}“`);
    });
    return lines.join("\n");
  }

  function issueUrl(list) {
    const text = exportText(list);
    const title = `Feedback: ${list.filter((e) => e.up).length}× 👍, ${list.filter((e) => !e.up).length}× 👎`;
    let body = `${text}\n\n<!-- techno-feedback v2 -->`;
    let url = `https://github.com/${OWNER}/${REPO}/issues/new?labels=feedback&title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
    if (url.length > MAX_URL) {
      body = `${text.slice(0, 4000)}\n…(gekürzt – Rest bitte über „Kopieren“ einfügen)`;
      url = `https://github.com/${OWNER}/${REPO}/issues/new?labels=feedback&title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
    }
    return url;
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_) {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    }
  }

  function renderInbox() {
    const box = document.getElementById("fb-inbox");
    if (!box) return;
    const list = load();
    if (!list.length) {
      box.innerHTML = `<p class="meta">Noch nichts gesammelt. Tippe beim Hören auf 👍 oder 👎 – die Stelle wird sofort hier gespeichert.</p>`;
      return;
    }
    box.innerHTML = `
      <ol class="fb-list">${list.map((e, i) => `
        <li><span class="fb-icon">${e.up ? "👍" : "👎"}</span>
          <span><span class="fb-w">${esc(e.where)}</span>${e.comment ? `<br><span class="fb-c">„${esc(e.comment)}“</span>` : ""}</span>
          <button class="fb-del" data-i="${i}" aria-label="Eintrag löschen">✕</button></li>`).join("")}
      </ol>
      <div class="fb-actions fb-export">
        <button class="btn" id="fb-copy">Kopieren</button>
        ${navigator.share ? `<button class="btn" id="fb-share">Teilen</button>` : ""}
        <a class="btn" id="fb-issue" href="${esc(issueUrl(list))}" target="_blank" rel="noopener">Als GitHub-Issue</a>
      </div>
      <p class="fb-hint">Kopieren/Teilen und z.B. in den Chat mit Claude einfügen – oder als ein GitHub-Issue senden (dort „Create“ tippen). <button class="linkbtn" id="fb-clear">Liste leeren</button></p>`;
    box.querySelectorAll(".fb-del").forEach((b) => b.addEventListener("click", () => {
      const l = load();
      l.splice(parseInt(b.dataset.i, 10), 1);
      save(l);
      renderInbox();
    }));
    box.querySelector("#fb-copy").addEventListener("click", async () => {
      toast((await copy(exportText(load()))) ? "Kopiert – jetzt im Chat einfügen" : "Kopieren nicht möglich");
    });
    const share = box.querySelector("#fb-share");
    if (share) share.addEventListener("click", () => navigator.share({ title: "Techno-Feedback", text: exportText(load()) }).catch(() => {}));
    box.querySelector("#fb-clear").addEventListener("click", () => {
      if (confirm("Alle gesammelten Feedbacks löschen?")) {
        save([]);
        renderInbox();
      }
    });
  }

  // Migrate entries from the first version (stored but never sent).
  try {
    const old = JSON.parse(localStorage.getItem("techno-feedback-history") || "[]");
    if (old.length && !load().length) {
      save(old.reverse().map((e) => ({ at: e.at, up: e.up, comment: e.comment, where: where(e.ctx || {}), ctx: e.ctx, engine: "0.2.0", profile: "hard_techno_simple", build: "" })));
    }
  } catch (_) {}

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", renderInbox);
  else renderInbox();

  window.TechnoFeedback = { open, renderInbox };
})();
