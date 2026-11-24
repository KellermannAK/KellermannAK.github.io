// Listening feedback: thumbs up / down plus a comment, filed as a pre-filled GitHub issue.
// The moment is captured when the thumb is tapped, so writing the comment can take a while.
// No server, no token: the issue is created by the listener on github.com.
(function () {
  const OWNER = "KellermannAK";
  const REPO = "KellermannAK.github.io";
  const HISTORY_KEY = "techno-feedback-history";

  let build = { commit: "", version: { engine: "?", profile: "?" } };
  fetch("songs.json", { cache: "no-store" })
    .then((r) => r.json())
    .then((j) => { build = { commit: j.commit || "", version: j.version || build.version }; })
    .catch(() => {});

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

  function dialog() {
    let d = document.getElementById("fb-dialog");
    if (d) return d;
    d = document.createElement("dialog");
    d.id = "fb-dialog";
    d.innerHTML = `
      <form method="dialog" class="fb">
        <div class="fb-thumbs">
          <button type="button" class="fb-thumb" data-r="up" aria-label="Daumen hoch">👍</button>
          <button type="button" class="fb-thumb" data-r="down" aria-label="Daumen runter">👎</button>
        </div>
        <p class="fb-where"></p>
        <textarea rows="4" placeholder="Was genau? z.B. „klickendes Geräusch auf der 2“, „Bass zu leise“, „super Drop“"></textarea>
        <p class="fb-hint">„Senden“ öffnet ein vorausgefülltes GitHub-Issue – dort nur noch auf „Create“ tippen.</p>
        <div class="fb-actions">
          <button value="cancel" class="btn">Abbrechen</button>
          <button value="send" class="play fb-send">Senden</button>
        </div>
      </form>`;
    document.body.appendChild(d);
    d.querySelectorAll(".fb-thumb").forEach((b) =>
      b.addEventListener("click", () => setRating(d, b.dataset.r)));
    d.addEventListener("close", () => {
      if (d.returnValue === "send") send(d);
    });
    return d;
  }

  function setRating(d, r) {
    d.dataset.rating = r;
    d.querySelectorAll(".fb-thumb").forEach((b) => b.classList.toggle("on", b.dataset.r === r));
  }

  function rows(ctx) {
    const v = build.version;
    const r = [
      ["Quelle", ctx.source],
      ctx.setSeed !== undefined && ["Set-Seed", ctx.setSeed],
      ctx.mixSeed !== undefined && ["Mix", `#${ctx.mixSeed}`],
      ctx.songIndex !== undefined && ["Song", `${ctx.songIndex + 1}${ctx.songSeed !== undefined ? ` (Seed ${ctx.songSeed})` : ""}`],
      ctx.key && ["Tonart", `${ctx.key}${ctx.camelot ? ` (${ctx.camelot})` : ""}${ctx.bpm ? `, ${ctx.bpm} BPM` : ""}`],
      ctx.section && ["Abschnitt", `${ctx.section}${ctx.sectionBar ? `, Takt ${ctx.sectionBar}` : ""}`],
      ctx.songBar && ["Song-Takt", ctx.songBar],
      ctx.time && ["Zeit", ctx.time],
      ["Engine", `${v.engine} · Profil ${v.profile} · Build ${(build.commit || "?").slice(0, 7)}`],
    ];
    return r.filter(Boolean);
  }

  function open(ctx, rating) {
    const d = dialog();
    d.ctx = ctx;
    setRating(d, rating);
    d.querySelector("textarea").value = "";
    d.querySelector(".fb-where").innerHTML = rows(ctx)
      .filter(([k]) => k !== "Engine" && k !== "Quelle")
      .map(([k, val]) => `<span>${esc(k)}: <b>${esc(val)}</b></span>`)
      .join("");
    d.showModal();
    d.querySelector("textarea").focus();
  }

  function send(d) {
    const ctx = d.ctx;
    const up = d.dataset.rating === "up";
    const comment = d.querySelector("textarea").value.trim();
    const where = ctx.mixSeed !== undefined
      ? `Mix ${ctx.mixSeed} · ${ctx.time}`
      : `Seed ${ctx.songSeed} · Takt ${ctx.songBarNum ?? "?"}`;
    const title = `Feedback ${up ? "👍" : "👎"} ${where}${comment ? ": " + comment.slice(0, 50) : ""}`;
    const table = rows(ctx).map(([k, val]) => `| ${k} | ${String(val).replace(/\|/g, "/")} |`).join("\n");
    const repro = [];
    if (ctx.songSeed !== undefined && ctx.songBarNum) {
      repro.push(`techno render --seed ${ctx.songSeed} --start-bar ${Math.max(0, ctx.songBarNum - 3)} --bars 4`);
    }
    if (ctx.setSeed !== undefined) repro.push(`techno mix --seed ${ctx.setSeed} --songs ${ctx.songIndex + 1}`);
    if (ctx.mixSeed !== undefined) repro.push(`techno mix --seed ${ctx.mixSeed} --songs ${ctx.mixSongs}`);
    const body = [
      `**Bewertung:** ${up ? "👍 gut" : "👎 schlecht"}`,
      "",
      `**Kommentar:** ${comment || "–"}`,
      "",
      "| | |",
      "|---|---|",
      table,
      "",
      "Reproduzieren (Tonart kann im Set abweichen, Rhythmus/Instrumente nicht):",
      "```",
      ...repro,
      "```",
      "",
      "<!-- techno-feedback v1 -->",
    ].join("\n");
    try {
      const hist = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
      hist.unshift({ at: new Date().toISOString(), up, comment, ctx });
      localStorage.setItem(HISTORY_KEY, JSON.stringify(hist.slice(0, 50)));
    } catch (_) { /* storage unavailable */ }
    const url = `https://github.com/${OWNER}/${REPO}/issues/new?labels=feedback&title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
    window.open(url, "_blank", "noopener");
  }

  window.TechnoFeedback = { open };
})();
