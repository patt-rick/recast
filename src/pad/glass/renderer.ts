/** Draws one Frame onto the pad canvas with Canvas 2D (spec §4.4 rendering strategy). */
import type { Frame, SlotFrame } from "../frame";
import { BLUE } from "../frame";
import { CX, CY, SIZE, SYMBOL, circlePath } from "../geometry";
import { iconPath } from "../icons";
import type { GlassSource } from "./backdrop";

const SOLID = { dark: "rgba(43,43,48,0.92)", light: "rgba(242,242,245,0.92)" } as const;
const pathCache = new Map<string, Path2D>();
const P = (d: string) => {
  let p = pathCache.get(d);
  if (!p) {
    if (pathCache.size > 512) pathCache.clear();
    p = new Path2D(d);
    pathCache.set(d, p);
  }
  return p;
};

function rimGradient(ctx: CanvasRenderingContext2D) {
  const g = ctx.createLinearGradient(0, 0, SIZE, SIZE);
  g.addColorStop(0, "rgba(255,255,255,0.95)");
  g.addColorStop(0.42, "rgba(255,255,255,0.25)");
  g.addColorStop(0.58, "rgba(255,255,255,0.12)");
  g.addColorStop(1, "rgba(255,255,255,0.6)");
  return g;
}

interface Piece {
  path: Path2D;
  ox: number;
  oy: number;
  mag: number;
  opacity: number;
  blurMix: number;
  light: boolean;
  tint: string;
  glowAlpha: number;
  innerAlpha: number;
  rimAlpha: number;
}

function drawGlass(ctx: CanvasRenderingContext2D, dpr: number, g: GlassSource | null, solid: Frame["solid"], q: Piece, glow: Frame["glow"], rim: CanvasGradient) {
  if (q.opacity <= 0.001) return;
  // Shadow: 0 8 16 .16 and 0 1 2 .12 (canvas shadows ignore the transform, so scale by dpr).
  // With glass, opaque black fills cast it; solid casts the long shadow from its one and only fill.
  ctx.save();
  ctx.globalAlpha = q.opacity;
  ctx.fillStyle = g ? "#000" : SOLID[solid ?? "dark"];
  ctx.shadowColor = "rgba(0,0,0,0.16)";
  ctx.shadowBlur = 16 * dpr;
  ctx.shadowOffsetY = 8 * dpr;
  ctx.fill(q.path);
  if (g) {
    ctx.shadowColor = "rgba(0,0,0,0.12)";
    ctx.shadowBlur = 2 * dpr;
    ctx.shadowOffsetY = 1 * dpr;
    ctx.fill(q.path);
  }
  ctx.restore();

  ctx.save();
  ctx.clip(q.path);
  if (g) {
    ctx.translate(q.ox, q.oy);
    ctx.scale(q.mag, q.mag);
    ctx.translate(-q.ox, -q.oy);
    // Cross-fade raw to blurred (deviation 6): once materialized, no sharp backdrop shows through.
    ctx.globalAlpha = q.opacity * (1 - q.blurMix);
    if (ctx.globalAlpha > 0.001) ctx.drawImage(g.raw, 0, 0, SIZE, SIZE);
    ctx.globalAlpha = q.opacity * q.blurMix;
    ctx.drawImage(q.light ? g.light : g.dark, 0, 0, SIZE, SIZE);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.globalAlpha = 1;
  }
  ctx.fillStyle = q.tint;
  ctx.fill(q.path);
  if (glow && q.glowAlpha > 0.001) {
    const rg = ctx.createRadialGradient(glow.x, glow.y, 0, glow.x, glow.y, 80);
    rg.addColorStop(0, "rgba(255,255,255,0.45)");
    rg.addColorStop(1, "rgba(255,255,255,0)");
    ctx.globalAlpha = q.glowAlpha;
    ctx.fillStyle = rg;
    ctx.fill(q.path);
    ctx.globalAlpha = 1;
  }
  // Inner light: a thin clipped stroke, softened with shadowBlur (deviation 5; thinned at the owner's request).
  const inner = (q.innerAlpha * 0.6).toFixed(3);
  ctx.lineWidth = 3;
  ctx.strokeStyle = `rgba(255,255,255,${inner})`;
  ctx.shadowColor = `rgba(255,255,255,${inner})`;
  ctx.shadowBlur = 2 * dpr;
  ctx.stroke(q.path);
  ctx.restore();

  ctx.save();
  ctx.globalAlpha = q.rimAlpha;
  ctx.lineWidth = 0.8;
  ctx.strokeStyle = rim;
  ctx.stroke(q.path);
  if (!g && solid === "light") {
    ctx.lineWidth = 0.5;
    ctx.strokeStyle = "rgba(0,0,0,0.12)";
    ctx.stroke(q.path);
  }
  ctx.restore();
}

function drawIcon(ctx: CanvasRenderingContext2D, name: string, x: number, y: number, size: number, color: string) {
  ctx.save();
  ctx.translate(x - size / 2, y - size / 2);
  ctx.scale(size / 24, size / 24);
  ctx.lineWidth = 1.75;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.strokeStyle = color;
  ctx.stroke(P(iconPath(name)));
  ctx.restore();
}

function slotIcon(s: SlotFrame): { icon: string; label: string } | null {
  switch (s.slot.kind) {
    case "preset":
      return { icon: s.slot.preset.icon, label: s.slot.preset.label };
    case "more":
      return { icon: "more", label: "More" };
    case "back":
      return { icon: "back", label: "Back" };
    default:
      return null;
  }
}

/** Fitted label font size per label text. */
const labelFonts = new Map<string, number>();

function drawSymbol(ctx: CanvasRenderingContext2D, s: SlotFrame, dpr: number) {
  const what = slotIcon(s);
  if (!what || s.symbolAlpha <= 0.001) return;
  ctx.save();
  ctx.globalAlpha = s.symbolAlpha;
  ctx.translate(s.sx, s.sy);
  ctx.scale(s.symbolScale, s.symbolScale);
  if (s.symbolColor === "#fff") {
    ctx.shadowColor = "rgba(0,0,0,0.3)";
    ctx.shadowBlur = 2 * dpr * s.symbolScale;
    ctx.shadowOffsetY = 1 * dpr * s.symbolScale;
  }
  drawIcon(ctx, what.icon, 0, SYMBOL.iconY, SYMBOL.iconSize, s.symbolColor);
  ctx.fillStyle = s.symbolColor;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  // A long label steps down a size rather than reaching past the tile's edge (worked out once per label).
  const fontOf = (px: number) => `600 ${px}px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`;
  let font = labelFonts.get(what.label);
  if (font == null) {
    font = SYMBOL.labelFont;
    ctx.font = fontOf(font);
    while (font > 9 && ctx.measureText(what.label).width > SYMBOL.labelMaxW) {
      font -= 0.5;
      ctx.font = fontOf(font);
    }
    labelFonts.set(what.label, font);
  }
  ctx.font = fontOf(font);
  ctx.fillText(what.label, 0, SYMBOL.labelY, SYMBOL.labelMaxW);
  ctx.restore();
  if (s.badge) {
    // Top-right of the icon, in screen space, so it never covers the label.
    const k = s.symbolScale;
    const bx = s.sx + SYMBOL.badgeDx * k;
    const by = s.sy + SYMBOL.badgeDy * k;
    ctx.save();
    ctx.globalAlpha = s.symbolAlpha;
    ctx.fillStyle = s.badge === "warning" ? "rgba(255,159,10,0.95)" : `rgba(${BLUE},0.95)`;
    ctx.beginPath();
    ctx.arc(bx, by, SYMBOL.badgeR * k, 0, Math.PI * 2);
    ctx.fill();
    drawIcon(ctx, s.badge === "warning" ? "warning" : "download", bx, by, 9.5 * k, "#fff");
    ctx.restore();
  }
}

/** Paint a frame. The canvas backing store is SIZE·dpr square. */
export function draw(ctx: CanvasRenderingContext2D, frame: Frame, glass: GlassSource | null, dpr: number) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const rim = rimGradient(ctx);
  const d = frame.disc;
  // No centre card: the icon and name sit straight on the backdrop. The disc is drawn only
  // with no tiles (the waiting pad, nothing to convert: it is then the drop target and holds
  // the note) or as the solid fill when there is no backdrop.
  if (d.drawn) {
    drawGlass(ctx, dpr, glass, frame.solid, {
      path: P(circlePath(CX, CY, d.r)), ox: CX, oy: CY, mag: d.mag, opacity: d.m, blurMix: d.m, light: d.light,
      tint: `rgba(255,255,255,${((d.light ? 0.35 : 0.06) * d.m).toFixed(3)})`,
      glowAlpha: 0, innerAlpha: 0.3 * d.m, rimAlpha: d.m,
    }, null, rim);
  }
  for (const s of frame.slots) {
    drawGlass(ctx, dpr, glass, frame.solid, {
      path: P(s.path), ox: s.ox, oy: s.oy, mag: s.mag, opacity: s.opacity, blurMix: s.blurMix, light: s.light,
      tint: s.tint, glowAlpha: s.glowAlpha, innerAlpha: s.innerAlpha, rimAlpha: s.rimAlpha,
    }, frame.glow, rim);
    if (s.focused) {
      ctx.save();
      ctx.lineWidth = 2.5;
      ctx.strokeStyle = `rgba(${BLUE},0.95)`;
      ctx.stroke(P(s.path));
      ctx.restore();
    }
    drawSymbol(ctx, s, dpr);
  }
  if (frame.ring) {
    const rr = d.r + 2.5; // in the 5 px between the disc and the ring
    ctx.save();
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.strokeStyle = d.light ? "rgba(0,0,0,0.12)" : "rgba(255,255,255,0.25)";
    ctx.beginPath();
    ctx.arc(CX, CY, rr, 0, Math.PI * 2);
    ctx.stroke();
    ctx.strokeStyle = `rgb(${BLUE})`;
    ctx.beginPath();
    if (frame.ring.pct != null) {
      ctx.arc(CX, CY, rr, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.01, frame.ring.pct));
    } else {
      const a = frame.ring.spin - Math.PI / 2;
      ctx.arc(CX, CY, rr, a, a + Math.PI / 2);
    }
    ctx.stroke();
    ctx.restore();
  }
}

/** Development check of the 4 ms budget (spec §4.4): ?perf in the pad URL logs mean draw time. */
export function perfProbe(enabled: boolean) {
  let total = 0, n = 0, last = performance.now();
  return <T>(fn: () => T): T => {
    if (!enabled) return fn();
    const t0 = performance.now();
    const out = fn();
    total += performance.now() - t0;
    n++;
    if (t0 - last > 2000) {
      console.debug(`[pad] mean draw ${(total / n).toFixed(2)} ms over ${n} frames`);
      total = 0;
      n = 0;
      last = t0;
    }
    return out;
  };
}
