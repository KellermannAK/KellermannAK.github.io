// Listening feedback: thumbs up / down or an idea (neutral), plus an optional comment.
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

  const ICON = { up: "👍", down: "👎", idea: "💡" };
  // Entries from before ratings existed only have `up`.
  const ratingOf = (e) => e.rating || (e.up ? "up" : "down");

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

  function load() {
    try { return JSON.parse(localStorage.getItem(KEY) || "[]"); } catch (_) { return []; }
  }
  function save(list) {
    try { localStorage.setItem(KEY, JSON.stringify(list)); } catch (_) { /* storage unavailable */ }
  }

  const genres = () => (window.TechnoGenre ? window.TechnoGenre.list : [{ id: "techno", name: "Techno", profile: "hard_techno_simple" }]);
  function genreOf(ctx) {
    const id = (ctx && ctx.genre) || (window.TechnoGenre ? window.TechnoGenre.current().id : "techno");
    return genres().find((g) => g.id === id) || null;
  }

  function where(ctx) {
    const parts = [];
    if (ctx.mixSeed !== undefined) parts.push(`Mix ${ctx.mixSeed} @ ${ctx.time}`);
    else if (ctx.favorites) parts.push(`Favorites mix @ ${ctx.time}`);
    else parts.push(`Live set ${ctx.setSeed} @ ${ctx.time}`);
    if (ctx.songIndex !== undefined) parts.push(`Song ${ctx.songIndex + 1}${ctx.title ? ` “${ctx.title}”` : ""}${ctx.songSeed !== undefined ? ` (Seed ${ctx.songSeed})` : ""}`);
    if (ctx.section) parts.push(`${ctx.section}${ctx.sectionBar ? ` ${ctx.sectionBar}` : ""}`);
    if (ctx.songBar) parts.push(`Song bar ${ctx.songBar}`);
    if (ctx.key) parts.push(`${ctx.key}${ctx.camelot ? ` (${ctx.camelot})` : ""}`);
    // The genre is always part of the position (the analysis rebuilds songs with it).
    const g = genreOf(ctx);
    if (g) parts.push(`Genre ${g.name}`);
    return parts.join(" · ");
  }

  const dbEnabled = () => !!(window.TechnoDB && window.TechnoDB.enabled);

  /** Retry entries that could not be stored in the database earlier. */
  async function syncPending() {
    if (!dbEnabled()) return;
    const pending = load();
    if (!pending.length) return;
    let stored = 0;
    for (const e of pending) {
      if (await window.TechnoDB.saveFeedback(e)) {
        save(load().filter((x) => x.at !== e.at));
        stored++;
      }
    }
    if (stored) {
      renderInbox();
      toast(`${stored} saved feedback${stored === 1 ? "" : "s"} sent ✓`);
    }
  }
  // The database module loads asynchronously; sync shortly after start.
  setTimeout(syncPending, 2500);

  /** Short message at the bottom; optional action button (e.g. "Undo"). */
  function toast(msg, actionLabel, action) {
    let t = document.getElementById("fb-toast");
    if (!t) {
      t = document.createElement("div");
      t.id = "fb-toast";
      document.body.appendChild(t);
    }
    t.textContent = msg;
    if (actionLabel) {
      const b = document.createElement("button");
      b.className = "linkbtn fb-toast-action";
      b.textContent = actionLabel;
      b.addEventListener("click", () => {
        t.classList.remove("show");
        action();
      });
      t.appendChild(b);
    }
    t.classList.add("show");
    clearTimeout(t._h);
    t._h = setTimeout(() => t.classList.remove("show"), actionLabel ? 6000 : 2200);
  }

  /** Remove sent entries, with a few seconds to undo. */
  function removeSent(sent) {
    const keys = new Set(sent.map((e) => e.at));
    const before = load();
    save(before.filter((e) => !keys.has(e.at)));
    renderInbox();
    toast(`${sent.length} sent ${sent.length === 1 ? "entry" : "entries"} removed`, "Undo", () => {
      save(before);
      renderInbox();
    });
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
          <button type="button" class="fb-thumb" data-r="up" aria-label="Thumbs up">👍</button>
          <button type="button" class="fb-thumb" data-r="down" aria-label="Thumbs down">👎</button>
          <button type="button" class="fb-thumb" data-r="idea" aria-label="Idea">💡</button>
        </div>
        <p class="fb-where"></p>
        <textarea rows="4" placeholder="Optional: what exactly? e.g. “clicking noise”, “great drop” – or your idea for 💡"></textarea>
        <div class="fb-actions">
          <button type="button" class="btn fb-cancel">Cancel</button>
          <button type="button" class="play fb-save">Save</button>
        </div>
      </div>`;
    document.body.appendChild(d);
    d.querySelectorAll(".fb-thumb").forEach((b) => b.addEventListener("click", () => setRating(d, b.dataset.r)));
    d.querySelector(".fb-cancel").addEventListener("click", () => d.close());
    d.querySelector(".fb-save").addEventListener("click", () => {
      const list = load();
      list.push({
        at: new Date().toISOString(),
        rating: d.dataset.rating,
        up: d.dataset.rating === "up",
        comment: d.querySelector("textarea").value.trim(),
        where: where(d.ctx),
        ctx: d.ctx,
        engine: build.version.engine,
        genre: d.ctx.genre,
        profile: (genreOf(d.ctx) || {}).profile || build.version.profile,
        // Mixes from the archive carry the build they were rendered with.
        build: d.ctx.build || (build.commit || "").slice(0, 7),
      });
      save(list);
      d.close();
      renderInbox();
      const entry = list[list.length - 1];
      if (dbEnabled()) {
        // Stored in the database: drop the local copy. On failure it stays and is retried.
        window.TechnoDB.saveFeedback(entry).then((ok) => {
          if (ok) {
            save(load().filter((x) => x.at !== entry.at));
            renderInbox();
            toast("Saved ✓");
          } else {
            toast("Saved locally – will be sent later");
          }
        });
      } else {
        toast(`Saved – ${list.length} feedback${list.length === 1 ? "" : "s"} collected`);
      }
    });
    return d;
  }

  function setRating(d, r) {
    d.dataset.rating = r;
    d.querySelectorAll(".fb-thumb").forEach((b) => b.classList.toggle("on", b.dataset.r === r));
  }

  function open(ctx, rating) {
    const d = dialog();
    // Every feedback carries its genre.
    d.ctx = { ...ctx, genre: (genreOf(ctx) || { id: "techno" }).id };
    setRating(d, rating);
    d.querySelector("textarea").value = "";
    d.querySelector(".fb-where").textContent = where(d.ctx);
    d.showModal();
  }

  // ------------------------------------------------------------------ inbox / export

  function exportText(list) {
    const v = list.length ? list[list.length - 1] : { engine: build.version.engine, profile: build.version.profile, build: "" };
    const lines = [`Feedback (${list.length}) – Engine ${v.engine} · Profile ${v.profile} · Build ${v.build || "?"}`, ""];
    list.forEach((e, i) => {
      lines.push(`${i + 1}. ${ICON[ratingOf(e)]} ${e.where}${e.build ? ` · Build ${e.build}` : ""}`);
      if (e.comment) lines.push(`   "${e.comment}"`);
    });
    return lines.join("\n");
  }

  function issueUrl(list, part, parts) {
    const text = exportText(list);
    const n = (r) => list.filter((e) => ratingOf(e) === r).length;
    const suffix = parts > 1 ? ` (part ${part}/${parts})` : "";
    const title = `Feedback: ${n("up")}× 👍, ${n("down")}× 👎, ${n("idea")}× 💡${suffix}`;
    const body = `${text}\n\n<!-- techno-feedback v2 -->`;
    return `https://github.com/${OWNER}/${REPO}/issues/new?labels=feedback&title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
  }

  /** Split the list into chunks whose issue URL stays below GitHub's length limit. */
  function issueParts(list) {
    const parts = [];
    let cur = [];
    for (const e of list) {
      const next = cur.concat([e]);
      if (cur.length && issueUrl(next, 9, 9).length > MAX_URL) {
        parts.push(cur);
        cur = [e];
      } else {
        cur = next;
      }
    }
    if (cur.length) parts.push(cur);
    return parts;
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
    // With the database the list is normally empty (entries are stored right away): hide the
    // whole section then, so the page stays about the music.
    const title = document.getElementById("fb-title");
    const hide = !list.length && dbEnabled();
    box.hidden = hide;
    if (title) title.hidden = hide;
    if (!list.length) {
      box.innerHTML = hide ? "" : `<p class="meta">Nothing collected yet. Tap 👍 or 👎 while listening – the moment is saved here right away.</p>`;
      return;
    }
    const parts = issueParts(list);
    box.innerHTML = `
      <ol class="fb-list">${list.map((e, i) => `
        <li><span class="fb-icon">${ICON[ratingOf(e)]}</span>
          <span><span class="fb-w">${esc(e.where)}</span>${e.comment ? `<br><span class="fb-c">“${esc(e.comment)}”</span>` : ""}</span>
          <button class="fb-del" data-i="${i}" aria-label="Delete entry">✕</button></li>`).join("")}
      </ol>
      <div class="fb-actions fb-export">
        <button class="btn" id="fb-copy">Copy</button>
        ${navigator.share ? `<button class="btn" id="fb-share">Share</button>` : ""}
        ${parts.map((p, i) => `<a class="btn fb-issue" data-part="${i}" href="${esc(issueUrl(p, i + 1, parts.length))}" target="_blank" rel="noopener">As GitHub issue${parts.length > 1 ? ` (part ${i + 1}/${parts.length})` : ""}</a>`).join("")}
      </div>
      <p class="fb-hint">Copy/share and paste into the chat with Claude – or send as a GitHub issue (tap “Create” there). Sent entries disappear from the list afterwards. <button class="linkbtn" id="fb-clear">Clear list</button></p>`;
    box.querySelectorAll(".fb-del").forEach((b) => b.addEventListener("click", () => {
      const l = load();
      l.splice(parseInt(b.dataset.i, 10), 1);
      save(l);
      renderInbox();
    }));
    box.querySelector("#fb-copy").addEventListener("click", async () => {
      const l = load();
      if (await copy(exportText(l))) removeSent(l);
      else toast("Copying not possible");
    });
    const share = box.querySelector("#fb-share");
    if (share) {
      share.addEventListener("click", () => {
        const l = load();
        navigator.share({ title: "Generator feedback", text: exportText(l) }).then(() => removeSent(l)).catch(() => {});
      });
    }
    box.querySelectorAll(".fb-issue").forEach((a) => a.addEventListener("click", () => {
      const sent = parts[parseInt(a.dataset.part, 10)];
      // Let the link open first, then drop the sent entries.
      setTimeout(() => removeSent(sent), 400);
    }));
    box.querySelector("#fb-clear").addEventListener("click", () => {
      if (confirm("Delete all collected feedback?")) {
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
    // Migrate only once: otherwise clearing the list would bring the old entries back.
    localStorage.removeItem("techno-feedback-history");
  } catch (_) {}

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", renderInbox);
  else renderInbox();

  window.TechnoFeedback = { open, renderInbox };
})();
