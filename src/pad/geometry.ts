/** Ring geometry (spec §4.4) in logical px of the 440×440 pad. */
export const SIZE = 440;
export const CX = SIZE / 2;
export const CY = SIZE / 2;
export const R = 136;
export const TH = 68;
export const GAP = 9;
export const RC = 17;
export const N = 6;
export const R0 = R - TH / 2;
export const R1 = R + TH / 2;
export const DISC_R = R0 - 14;
export const STEP = (2 * Math.PI) / N;
/** Segment lift when targeted, px. */
export const LIFT = 9;

/** Centre angle of slot i: slot 0 at 12 o'clock, clockwise. */
export const slotAngle = (i: number) => -Math.PI / 2 + i * STEP;

/** Signed smallest difference a − b, in (−π, π]. */
export const angleDiff = (a: number, b: number) => Math.atan2(Math.sin(a - b), Math.cos(a - b));

const f = (n: number) => n.toFixed(2);

/**
 * Annular sector with parallel gaps and rounded corners (the mockup's `rsector`).
 * ca: centre angle, hs: half span, gap: total gap between neighbours, rc: corner radius.
 */
export function rsector(cx: number, cy: number, r: number, th: number, ca: number, hs: number, gap: number, rc: number): string {
  const ro = r + th / 2;
  const ri = r - th / 2;
  const ao = Math.asin(gap / 2 / ro);
  const ai = Math.asin(gap / 2 / ri);
  const a0o = ca - hs + ao, a1o = ca + hs - ao, a0i = ca - hs + ai, a1i = ca + hs - ai;
  const P = (rad: number, a: number) => `${f(cx + rad * Math.cos(a))} ${f(cy + rad * Math.sin(a))}`;
  const L = (a: number, b: number, t: number) => a + (b - a) * t;
  const lgo = a1o - a0o > Math.PI ? 1 : 0;
  const dao = rc / ro, dai = rc / ri;
  const e1 = (rad: number) => L(a1i, a1o, (rad - ri) / (ro - ri));
  const e0 = (rad: number) => L(a0i, a0o, (rad - ri) / (ro - ri));
  return (
    `M${P(ro, a0o + dao)} A${ro} ${ro} 0 ${lgo} 1 ${P(ro, a1o - dao)} Q${P(ro, a1o)} ${P(ro - rc, e1(ro - rc))} L${P(ri + rc, e1(ri + rc))} Q${P(ri, a1i)} ${P(ri, a1i - dai)}` +
    ` A${ri} ${ri} 0 ${lgo} 0 ${P(ri, a0i + dai)} Q${P(ri, a0i)} ${P(ri + rc, e0(ri + rc))} L${P(ro - rc, e0(ro - rc))} Q${P(ro, a0o)} ${P(ro, a0o + dao)} Z`
  );
}

/** A circle as path data (the disc). */
export const circlePath = (cx: number, cy: number, r: number) =>
  `M${cx - r} ${cy} a${r} ${r} 0 1 0 ${2 * r} 0 a${r} ${r} 0 1 0 ${-2 * r} 0Z`;

/**
 * Point inside the sector (ignoring the rounded corners): within the radial band,
 * on the slot's side of both boundary rays and at least gap/2 from each.
 */
export function inSector(x: number, y: number, cx: number, cy: number, r: number, th: number, ca: number, hs: number, gap: number): boolean {
  const dx = x - cx, dy = y - cy;
  const rad = Math.hypot(dx, dy);
  if (rad < r - th / 2 || rad > r + th / 2) return false;
  const rel = angleDiff(Math.atan2(dy, dx), ca);
  if (Math.abs(rel) > hs) return false;
  const toEdge = rad * Math.sin(hs - Math.abs(rel));
  return toEdge >= gap / 2;
}
