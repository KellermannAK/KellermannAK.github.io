// Record labels for the visuals mode: every song gets its own look, drawn from its seed, so
// the same song always has the same label. A label is a layout of artist and title (curved
// along the rim, straight, on a diagonal band, split in two halves ...), a colour scheme, a
// type style and sometimes a cheap, simple decoration (rings, dots, bars, a triangle ...).
// Coordinates are those of the 220 x 220 turntable drawing; the label is a circle of radius 58
// around (110, 110). The caller puts the result into a group that spins with the record.

const C = 110;
const R = 58;

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const f = (n) => Number(n).toFixed(2);
const pt = (r, deg) => {
  const a = (deg * Math.PI) / 180;
  return [C + r * Math.sin(a), C - r * Math.cos(a)];
};

// Small seeded generator (mulberry32).
function rng(seed) {
  let a = (Math.imul(Number(seed) || 1, 2654435761) ^ 0x9e3779b9) >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const pick = (r, list) => list[Math.floor(r() * list.length) % list.length];

// [background, ink, accent shape, ink on the accent shape]. "var(...)" colours follow the
// genre theme.
const SCHEMES = [
  ["var(--accent)", "var(--label-ink)", "var(--accent2)", "#1b1b20"],
  ["var(--accent2)", "#1b1b20", "var(--accent)", "#1b1b20"],
  ["#f1e6c8", "#2b2217", "#c0462f", "#ffffff"],
  ["#e9e9ee", "#18181d", "#3a6ea5", "#ffffff"],
  ["#2b2b33", "#f1f1f6", "#ffb13d", "#1b1b20"],
  ["#2f8f83", "#ffffff", "#f1e6c8", "#2b2217"],
  ["#d9a521", "#2a1f00", "#2b2b33", "#ffffff"],
  ["#d9482b", "#ffffff", "#f1e6c8", "#2b2217"],
  ["#7a5aa6", "#ffffff", "#ffd36b", "#2a1f00"],
];

const FONTS = [
  { family: "system-ui, sans-serif", weight: 700, style: "normal", spacing: 0, caps: false },
  { family: "system-ui, sans-serif", weight: 800, style: "normal", spacing: 1.2, caps: true },
  { family: "Georgia, 'Times New Roman', serif", weight: 700, style: "italic", spacing: 0, caps: false },
  { family: "Georgia, 'Times New Roman', serif", weight: 400, style: "normal", spacing: 0.6, caps: true },
  { family: "ui-monospace, Menlo, Consolas, monospace", weight: 700, style: "normal", spacing: 0, caps: false },
  { family: "system-ui, sans-serif", weight: 300, style: "normal", spacing: 2, caps: true },
];

/** Text style as an inline CSS string. */
const css = (font, size, ink, weight = font.weight) =>
  `font-family:${font.family};font-weight:${weight};font-style:${font.style};font-size:${size}px;letter-spacing:${font.spacing}px;fill:${ink}`;

const clip = (s, n) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

/** Straight text, squeezed to the chord of the label at height y. */
function line(text, y, size, font, ink, extra = "") {
  const t = esc(font.caps ? text.toUpperCase() : text);
  const avail = 2 * Math.sqrt(Math.max(R * R - (y - C) * (y - C), 0)) - 12;
  const natural = (font.caps ? 0.68 : 0.56) * size * text.length + font.spacing * text.length;
  const fit = natural > avail ? ` textLength="${f(avail)}" lengthAdjust="spacingAndGlyphs"` : "";
  return `<text x="${C}" y="${f(y)}" text-anchor="middle" style="${css(font, size, ink)}"${fit}${extra}>${t}</text>`;
}

/** Text along an arc of the label: top (reads clockwise) or bottom (reads left to right). */
function arc(id, text, radius, span, bottom, size, font, ink) {
  const [x0, y0] = bottom ? pt(radius, 180 + span) : pt(radius, -span);
  const [x1, y1] = bottom ? pt(radius, 180 - span) : pt(radius, span);
  const length = ((radius * span * 2) / 180) * Math.PI;
  const t = esc(font.caps ? text.toUpperCase() : text);
  const natural = (font.caps ? 0.68 : 0.56) * size * text.length + font.spacing * text.length;
  const fit = natural > length ? ` textLength="${f(length)}" lengthAdjust="spacingAndGlyphs"` : "";
  return `<path id="${id}" d="M${f(x0)} ${f(y0)} A${radius} ${radius} 0 ${span > 90 ? 1 : 0} ${bottom ? 0 : 1} ${f(x1)} ${f(y1)}" fill="none"/>
    <text style="${css(font, size, ink)}"><textPath href="#${id}" startOffset="50%" text-anchor="middle"${fit}>${t}</textPath></text>`;
}

/** Cheap, simple decorations. They stay away from the text areas. */
function decoration(kind, ink, accent) {
  switch (kind) {
    case "ring":
      return `<circle cx="${C}" cy="${C}" r="52" fill="none" stroke="${ink}" stroke-width="0.8" opacity="0.55"/>
        <circle cx="${C}" cy="${C}" r="17" fill="none" stroke="${ink}" stroke-width="0.8" opacity="0.55"/>`;
    case "dots": {
      let d = "";
      for (let k = 0; k < 16; k++) {
        const [x, y] = pt(53.5, k * 22.5);
        d += `<circle cx="${f(x)}" cy="${f(y)}" r="1.3" fill="${ink}" opacity="0.7"/>`;
      }
      return d;
    }
    case "bars":
      return [-1, 1].map((s) => [0, 1, 2].map((k) => `<rect x="${C + s * 34 - (s < 0 ? 16 : 0)}" y="${C - 5 + k * 4.5}" width="16" height="1.8" fill="${ink}" opacity="0.6"/>`).join("")).join("");
    case "triangle":
      return `<path d="M${C} ${C - 22} L${C + 8} ${C - 9} L${C - 8} ${C - 9} Z" fill="${accent}"/>`;
    case "diamond":
      return `<path d="M${C} ${C - 16} L${C + 16} ${C} L${C} ${C + 16} L${C - 16} ${C} Z" fill="none" stroke="${ink}" stroke-width="1.1" opacity="0.7"/>`;
    case "square":
      return `<rect x="${C - 12}" y="${C - 12}" width="24" height="24" fill="none" stroke="${accent}" stroke-width="1.6" transform="rotate(18 ${C} ${C})"/>`;
    case "stripe":
      return `<rect x="${C - 54}" y="${C - 1}" width="108" height="2" fill="${accent}" opacity="0.9"/>`;
    default:
      return "";
  }
}

const DECORATIONS = ["none", "none", "ring", "dots", "bars", "triangle", "diamond", "square", "stripe"];
const LAYOUTS = ["arcs", "arcs", "straight", "titleArc", "band", "split", "minimal"];

/**
 * SVG for the label of one song. `uid` keeps the path ids unique (one per deck).
 * Empty artist / title give a plain label.
 */
export function labelSvg(uid, seed, artist, title) {
  const r = rng(seed);
  const [bg, ink, accent, onAccent] = pick(r, SCHEMES);
  const font = pick(r, FONTS);
  const layout = pick(r, LAYOUTS);
  let deco = pick(r, DECORATIONS);
  // Straight lines through the middle would clash with a stripe or bars of their own.
  if ((layout === "straight" || layout === "split") && (deco === "stripe" || deco === "bars")) deco = "ring";
  const a = clip(String(artist || ""), 34);
  const t = clip(String(title || ""), 34);
  const base = `<circle cx="${C}" cy="${C}" r="${R}" style="fill:${bg}"/>`;
  if (!a && !t) return base;
  let body = "";
  const id = (n) => `lb-${uid}-${n}`;
  switch (layout) {
    case "straight": {
      const tilt = (r() - 0.5) * 28;
      body = `<g transform="rotate(${f(tilt)} ${C} ${C})">
        ${line(a, C - 13, 8.5, font, ink, ' opacity="0.9"')}
        <rect x="${C - 24}" y="${C - 3}" width="48" height="1.4" fill="${ink}" opacity="0.7"/>
        ${line(t, C + 17, 12, font, ink)}</g>`;
      break;
    }
    case "titleArc":
      body = `${arc(id(1), t, 45, 100, false, 11, font, ink)}${line(a, C + 30, 8.5, font, ink, ' opacity="0.9"')}`;
      break;
    case "band": {
      const angle = pick(r, [-24, -16, 16, 24]);
      body = `<g transform="rotate(${angle} ${C} ${C})">
          <rect x="${C - R}" y="${C - 34}" width="${2 * R}" height="24" style="fill:${accent}"/>
          ${line(t, C - 18, 11, font, onAccent)}</g>
        ${arc(id(1), a, 46, 80, true, 8.5, font, ink)}`;
      break;
    }
    case "split": {
      const [x0, y0] = pt(R, -90);
      const [x1, y1] = pt(R, 90);
      body = `<path d="M${f(x0)} ${f(y0)} A${R} ${R} 0 0 1 ${f(x1)} ${f(y1)} Z" style="fill:${accent}" opacity="0.9"/>
        ${line(a, C - 22, 9, font, onAccent)}
        ${line(t, C + 30, 10.5, font, ink)}`;
      break;
    }
    case "minimal":
      body = `${line(t, C + 24, 14, font, ink)}${arc(id(1), a, 47, 60, false, 8, font, ink)}`;
      break;
    default: // "arcs"
      body = `${arc(id(1), a, 43, 85, false, 9.5, font, ink)}${arc(id(2), t, 46, 85, true, 9.5, font, ink)}`;
  }
  // Everything stays inside the label (bands and halves are cut at its edge).
  return `${base}<clipPath id="lc-${uid}"><circle cx="${C}" cy="${C}" r="${R}"/></clipPath>
    <g clip-path="url(#lc-${uid})">${decoration(deco, ink, accent)}${body}</g>`;
}
