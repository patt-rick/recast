/** One frame of the ring as numbers (spec §4.4, the mockup's regular()). Pure. */
import { approach, clamp01, easeOutCubic, lerp, materialize, PAGE_FADE_MS } from "./anim";
import { annulusPath, CX, CY, DISC_R, EMPTY_DISC_R, GAP, LIFT, MAX_SLOTS, R, RC, TH, rsector, slotAngle, step } from "./geometry";
import { isLight } from "./luminance";
import type { Slot } from "./paging";

export const BLUE = "10,132,255";
export interface Spring { h: number; off: number }
/** One spring per slot index, for the largest page; slots a page doesn't have stay at rest. */
export const restSprings = (): Spring[] => Array.from({ length: MAX_SLOTS }, () => ({ h: 0, off: 0 }));
/** A hovered slot widens by 4% of a sixth of the ring at most, so wide slots (2 or 3 on a
 * page) don't grow into their neighbours. */
const MAX_GROW = (Math.PI / MAX_SLOTS) * 0.04;

export function stepSprings(s: Spring[], target: number | null, dt: number, reduced: boolean): Spring[] {
  return s.map((sp, i) => {
    const on = target === i;
    return {
      h: on || sp.h > 0.001 ? approach(sp.h, on ? 1 : 0, dt) : 0,
      off: reduced ? 0 : on || sp.off > 0.01 ? approach(sp.off, on ? LIFT : 0, dt) : 0,
    };
  });
}

export const springsSettled = (s: Spring[], target: number | null) =>
  s.every((sp, i) => Math.abs(sp.h - (target === i ? 1 : 0)) < 0.002 && Math.abs(sp.off - (target === i ? LIFT : 0)) < 0.02);

export interface FrameInput {
  now: number;
  openedAt: number;
  page: Slot[];
  pageAt: number;
  target: number | null;
  focus: number | null;
  showFocus: boolean;
  springs: Spring[];
  /** Per-slot backdrop luminance; null when there's no backdrop. */
  lum: number[] | null;
  discLum: number | null;
  /** The reduced-transparency surface when there is no backdrop. */
  solid: "dark" | "light" | null;
  reducedMotion: boolean;
  pointer: { x: number; y: number } | null;
  /** 0..1: segments retract into the disc while running. */
  retract: number;
  progress: { pct: number | null; spin: number } | null;
}

export interface SlotFrame {
  index: number;
  slot: Slot;
  path: string;
  /** Glass magnification origin and symbol centre (lifted). */
  sx: number;
  sy: number;
  /** Glass magnification origin: the symbol centre, or the ring's centre for a one-slot page. */
  ox: number;
  oy: number;
  mag: number;
  /** Whole-piece opacity: open animation x retract. */
  opacity: number;
  /** Raw -> blurred cross-fade of the open animation (no retract). */
  blurMix: number;
  light: boolean;
  tint: string;
  glowAlpha: number;
  innerAlpha: number;
  rimAlpha: number;
  symbolColor: string;
  symbolScale: number;
  symbolAlpha: number;
  focused: boolean;
  badge: "download" | "warning" | null;
}

export interface Frame {
  slots: SlotFrame[];
  /** The centre. Drawn only with no tiles, or as the solid fill without a backdrop. */
  disc: { r: number; m: number; mag: number; light: boolean; drawn: boolean };
  glow: { x: number; y: number } | null;
  solid: "dark" | "light" | null;
  ring: { pct: number | null; spin: number } | null;
}

export function buildFrame(f: FrameInput): Frame {
  const elapsed = f.now - f.openedAt;
  const fadeIn = f.reducedMotion ? 1 : easeOutCubic((f.now - f.pageAt) / PAGE_FADE_MS);
  const keep = 1 - f.retract;
  const n = f.page.length;
  const whole = n === 1; // one slot: the whole ring
  const st = step(n);
  const slots = f.page.map((slot, i): SlotFrame => {
    const mo = materialize(elapsed, i + 1, f.reducedMotion);
    const dropped = keep < 0.2;
    const m = dropped ? 0 : mo * keep;
    const sp = f.springs[i] ?? { h: 0, off: 0 };
    const h = sp.h;
    const hg = f.reducedMotion ? 0 : h; // geometry follows the spring only with motion
    const ca = slotAngle(i, n);
    const hs = (st / 2 + Math.min(0.04 * (st / 2), MAX_GROW) * hg) * lerp(0.55, 1, mo);
    const th = TH * (1 + 0.08 * hg) * lerp(0.8, 1, mo);
    // A whole ring isn't lifted (that would only shift it) and has no arc to grow:
    // only its thickness and opacity animate.
    const off = f.reducedMotion || whole ? 0 : sp.off;
    const r = lerp(DISC_R, R, keep);
    const cx = CX + off * Math.cos(ca), cy = CY + off * Math.sin(ca);
    const t = Math.max(0.01, th);
    const light = f.lum ? isLight(f.lum[i]) : f.solid === "light";
    const tint = h > 0.02
      ? `rgba(${BLUE},${((light ? 0.8 : 0.45) * h).toFixed(3)})`
      : `rgba(255,255,255,${((light ? 0.12 : 0.06) * m).toFixed(3)})`;
    const badge = slot.kind === "preset"
      ? slot.preset.engine_status.state !== "installed" ? "download" : slot.preset.warning ? "warning" : null
      : null;
    const sx = cx + r * Math.cos(ca), sy = cy + r * Math.sin(ca);
    return {
      index: i,
      slot,
      path: whole ? annulusPath(CX, CY, r + t / 2, Math.max(0, r - t / 2)) : rsector(cx, cy, r, t, ca, hs, GAP, RC),
      sx,
      sy,
      ox: whole ? CX : sx,
      oy: whole ? CY : sy,
      // Wide slots (3 or fewer on a page) magnify less: about their own centre, a big
      // magnification would make the backdrop jump at their edges.
      mag: lerp(1, (n <= 3 ? 1.04 : 1.1) + 0.05 * hg, mo),
      opacity: m,
      blurMix: mo,
      light,
      tint,
      glowAlpha: h * (light ? 0.3 : 0.9),
      innerAlpha: (light ? 0.6 : 0.32 + 0.2 * h) * m,
      rimAlpha: m,
      symbolColor: light && h < 0.5 ? "#1d1d1f" : "#fff",
      symbolScale: 1 + 0.1 * hg,
      symbolAlpha: m * fadeIn,
      focused: f.showFocus && f.focus === i,
      badge,
    };
  });
  const m0 = materialize(elapsed, 0, f.reducedMotion);
  return {
    slots,
    disc: {
      r: n === 0 ? EMPTY_DISC_R : DISC_R,
      m: m0,
      mag: lerp(1, 1.06, m0),
      light: f.discLum != null ? isLight(f.discLum) : f.solid === "light",
      drawn: n === 0 || f.solid != null,
    },
    glow: f.pointer,
    solid: f.solid,
    ring: f.progress,
  };
}

/** Running: segments slide into the disc over ~300 ms (instant with reduced motion). */
export const stepRetract = (cur: number, running: boolean, dt: number, reduced: boolean) =>
  reduced ? (running ? 1 : 0) : clamp01(approach(cur, running ? 1 : 0, dt, 0.25));
