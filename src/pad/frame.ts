/** One frame of the ring as numbers (spec §4.4, the mockup's regular()). Pure. */
import { approach, clamp01, easeOutCubic, lerp, materialize, PAGE_FADE_MS } from "./anim";
import { CX, CY, DISC_R, GAP, LIFT, N, R, RC, STEP, TH, rsector, slotAngle } from "./geometry";
import { isLight } from "./luminance";
import type { Slot } from "./paging";

export const BLUE = "10,132,255";
export interface Spring { h: number; off: number }
export const restSprings = (): Spring[] => Array.from({ length: N }, () => ({ h: 0, off: 0 }));

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
  mag: number;
  /** Whole-piece opacity: open animation x retract (0.5x for empty slots). */
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
  disc: { r: number; m: number; mag: number; light: boolean };
  glow: { x: number; y: number } | null;
  solid: "dark" | "light" | null;
  ring: { pct: number | null; spin: number } | null;
}

export function buildFrame(f: FrameInput): Frame {
  const elapsed = f.now - f.openedAt;
  const fadeIn = f.reducedMotion ? 1 : easeOutCubic((f.now - f.pageAt) / PAGE_FADE_MS);
  const keep = 1 - f.retract;
  const slots = f.page.map((slot, i): SlotFrame => {
    const mo = materialize(elapsed, i + 1, f.reducedMotion);
    const dropped = keep < 0.2;
    const m = dropped ? 0 : mo * keep;
    const sp = f.springs[i] ?? { h: 0, off: 0 };
    const h = sp.h;
    const hg = f.reducedMotion ? 0 : h; // geometry follows the spring only with motion
    const ca = slotAngle(i);
    const hs = (STEP / 2) * (1 + 0.04 * hg) * lerp(0.55, 1, mo);
    const th = TH * (1 + 0.08 * hg) * lerp(0.8, 1, mo);
    const off = f.reducedMotion ? 0 : sp.off;
    const r = lerp(DISC_R, R, keep);
    const cx = CX + off * Math.cos(ca), cy = CY + off * Math.sin(ca);
    const light = f.lum ? isLight(f.lum[i]) : f.solid === "light";
    const empty = slot.kind === "empty";
    const tint = h > 0.02
      ? `rgba(${BLUE},${((light ? 0.8 : 0.45) * h).toFixed(3)})`
      : `rgba(255,255,255,${((light ? 0.12 : 0.06) * m).toFixed(3)})`;
    const badge = slot.kind === "preset"
      ? slot.preset.engine_status.state !== "installed" ? "download" : slot.preset.warning ? "warning" : null
      : null;
    return {
      index: i,
      slot,
      path: rsector(cx, cy, r, Math.max(0.01, th), ca, hs, GAP, RC),
      sx: cx + r * Math.cos(ca),
      sy: cy + r * Math.sin(ca),
      mag: lerp(1, 1.1 + 0.05 * hg, mo),
      opacity: empty ? 0.5 * m : m,
      blurMix: mo,
      light,
      tint: empty ? `rgba(255,255,255,${(0.03 * m).toFixed(3)})` : tint,
      glowAlpha: empty ? 0 : h * (light ? 0.3 : 0.9),
      innerAlpha: (light ? 0.6 : 0.32 + 0.2 * h) * m,
      rimAlpha: m,
      symbolColor: light && h < 0.5 ? "#1d1d1f" : "#fff",
      symbolScale: 1 + 0.1 * hg,
      symbolAlpha: empty ? 0 : m * fadeIn,
      focused: f.showFocus && f.focus === i,
      badge,
    };
  });
  const m0 = materialize(elapsed, 0, f.reducedMotion);
  return {
    slots,
    disc: {
      r: DISC_R,
      m: m0,
      mag: lerp(1, 1.06, m0),
      light: f.discLum != null ? isLight(f.discLum) : f.solid === "light",
    },
    glow: f.pointer,
    solid: f.solid,
    ring: f.progress,
  };
}

/** Running: segments slide into the disc over ~300 ms (instant with reduced motion). */
export const stepRetract = (cur: number, running: boolean, dt: number, reduced: boolean) =>
  reduced ? (running ? 1 : 0) : clamp01(approach(cur, running ? 1 : 0, dt, 0.25));
