/** Open animation (spec §4.4): 120 ms delay, 55 ms stagger, 520 ms ease-out-cubic. */
export const OPEN_DELAY = 120;
export const STAGGER = 55;
export const OPEN_DUR = 520;
/** Disc (index 0) plus six slots (1..6). */
export const OPEN_MS = OPEN_DELAY + STAGGER * 6 + OPEN_DUR;
/** Cross-fade when the ring pages (More/Back). */
export const PAGE_FADE_MS = 240;

export const clamp01 = (t: number) => Math.max(0, Math.min(1, t));
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const easeOutCubic = (t: number) => 1 - Math.pow(1 - clamp01(t), 3);

/** 0..1 progress of piece `index` (0 = disc, 1..6 = slots) `elapsed` ms after opening. */
export function materialize(elapsed: number, index: number, reduced: boolean): number {
  if (reduced) return 1;
  return easeOutCubic((elapsed - OPEN_DELAY - index * STAGGER) / OPEN_DUR);
}

/** The mockup's per-frame lerp(…, 0.2), made independent of the frame rate. */
export function approach(cur: number, target: number, dtMs: number, rate = 0.2): number {
  const k = 1 - Math.pow(1 - rate, dtMs / 16.667);
  return cur + (target - cur) * k;
}
