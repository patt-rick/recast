/** Draws one Frame onto the pad canvas with Canvas 2D (spec §4.4 rendering strategy). */
import type { Frame, SlotFrame } from "../frame";
import { BLUE } from "../frame";
import { CX, CY, SIZE, TH, circlePath, slotAngle } from "../geometry";
import { iconPath, kindIcon } from "../icons";
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
    ctx.globalAlpha = q.opacity;
    ctx.drawImage(g.raw, 0, 0, SIZE, SIZE);
    ctx.globalAlpha = q.opacity * q.blurMix; // cross-fade raw to blurred (deviation 6)
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
  // Inner light: clipped 7 px stroke, softened with shadowBlur (deviation 5).
  ctx.lineWidth = 7;
  ctx.strokeStyle = `rgba(255,255,255,${q.innerAlpha.toFixed(3)})`;
  ctx.shadowColor = `rgba(255,255,255,${q.innerAlpha.toFixed(3)})`;
  ctx.shadowBlur = 4.4 * dpr;
  ctx.stroke(q.path);
  ctx.restore();

  ctx.save();
  ctx.globalAlpha = q.rimAlpha;
  ctx.lineWidth = 1.2;
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
  drawIcon(ctx, what.icon, 0, -8, 23, s.symbolColor);
  ctx.fillStyle = s.symbolColor;
  ctx.font = '600 11px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif';
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(what.label, 0, 15);
  ctx.restore();
  if (s.badge) {
    const ca = slotAngle(s.index);
    const bx = s.sx + (TH / 2 - 12) * Math.cos(ca) + 18 * Math.cos(ca + Math.PI / 2);
    const by = s.sy + (TH / 2 - 12) * Math.sin(ca) + 18 * Math.sin(ca + Math.PI / 2);
    ctx.save();
    ctx.globalAlpha = s.symbolAlpha;
    ctx.fillStyle = s.badge === "warning" ? "rgba(255,159,10,0.95)" : `rgba(${BLUE},0.95)`;
    ctx.beginPath();
    ctx.arc(bx, by, 8, 0, Math.PI * 2);
    ctx.fill();
    drawIcon(ctx, s.badge === "warning" ? "warning" : "download", bx, by, 11, "#fff");
    ctx.restore();
  }
}

/** Paint a frame. The canvas backing store is SIZE·dpr square. */
export function draw(ctx: CanvasRenderingContext2D, frame: Frame, glass: GlassSource | null, dpr: number, fallbackKind: string | null) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const rim = rimGradient(ctx);
  const d = frame.disc;
  drawGlass(ctx, dpr, glass, frame.solid, {
    path: P(circlePath(CX, CY, d.r)), ox: CX, oy: CY, mag: d.mag, opacity: d.m, blurMix: d.m, light: d.light,
    tint: `rgba(255,255,255,${((d.light ? 0.35 : 0.06) * d.m).toFixed(3)})`,
    glowAlpha: 0, innerAlpha: 0.3 * d.m, rimAlpha: d.m,
  }, null, rim);
  for (const s of frame.slots) {
    drawGlass(ctx, dpr, glass, frame.solid, {
      path: P(s.path), ox: s.sx, oy: s.sy, mag: s.mag, opacity: s.opacity, blurMix: s.blurMix, light: s.light,
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
    const rr = d.r + 7;
    ctx.save();
    ctx.lineWidth = 4;
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
  if (fallbackKind && d.m > 0.01) {
    drawIcon(ctx, kindIcon(fallbackKind), CX, CY - 22, 34, d.light ? "#1d1d1f" : "#fff");
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
