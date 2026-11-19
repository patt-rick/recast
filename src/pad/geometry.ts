/** Ring geometry (spec §4.4) in logical px of the 368×368 pad. */
export const SIZE = 368;
export const CX = SIZE / 2;
export const CY = SIZE / 2;
/** The tiles sit right around the icon (inner radius 64). */
export const R = 94;
export const TH = 60;
/** Between tiles: tight, so the ring reads as one piece cut into tiles (owner, 2026-11-19). */
export const GAP = 4;
export const RC = 8;
/** Most slots on one page; a page shows only the slots it has (1..6, or none). */
export const MAX_SLOTS = 6;
export const R0 = R - TH / 2;
export const R1 = R + TH / 2;
/**
 * The centre: icon, name and progress ring, 5 px inside the ring. It is drawn as glass only
 * when there are no tiles (the waiting pad, nothing to convert), and then larger
 * (`EMPTY_DISC_R`), or as the solid fill when there is no backdrop.
 */
export const DISC_R = R0 - 5;
export const EMPTY_DISC_R = R - 4;
/**
 * A tile's icon, label and badge, relative to its symbol centre (unscaled). Kept inside the
 * tile at every slot (geometry test).
 */
export const SYMBOL = { iconY: -7, iconSize: 21, labelY: 11, labelMaxW: 44, labelFont: 11, badgeDx: 15, badgeDy: -16, badgeR: 6.5 } as const;
/** Angular span of one slot on a page of n slots (n 0 counts as 1, so angles stay finite). */
export const step = (n: number) => (2 * Math.PI) / (n || 1);
/** Segment lift when targeted, px. */
export const LIFT = 9;

/** Centre angle of slot i of n: slot 0 at 12 o'clock, clockwise. */
export const slotAngle = (i: number, n: number) => -Math.PI / 2 + i * step(n);

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
  // Each arc's large-arc flag comes from the span it actually draws (the rounded corners
  // take rc/r off each end); outer and inner can differ near a half ring.
  const lgo = a1o - a0o - 2 * (rc / ro) > Math.PI ? 1 : 0;
  const lgi = a1i - a0i - 2 * (rc / ri) > Math.PI ? 1 : 0;
  const dao = rc / ro, dai = rc / ri;
  const e1 = (rad: number) => L(a1i, a1o, (rad - ri) / (ro - ri));
  const e0 = (rad: number) => L(a0i, a0o, (rad - ri) / (ro - ri));
  return (
    `M${P(ro, a0o + dao)} A${ro} ${ro} 0 ${lgo} 1 ${P(ro, a1o - dao)} Q${P(ro, a1o)} ${P(ro - rc, e1(ro - rc))} L${P(ri + rc, e1(ri + rc))} Q${P(ri, a1i)} ${P(ri, a1i - dai)}` +
    ` A${ri} ${ri} 0 ${lgi} 0 ${P(ri, a0i + dai)} Q${P(ri, a0i)} ${P(ri + rc, e0(ri + rc))} L${P(ro - rc, e0(ro - rc))} Q${P(ro, a0o)} ${P(ro, a0o + dao)} Z`
  );
}

/**
 * A full ring (a page with one slot): the outer circle clockwise and the inner one
 * anticlockwise, so the default nonzero fill rule leaves the hole.
 */
export function annulusPath(cx: number, cy: number, ro: number, ri: number): string {
  return (
    `M${f(cx + ro)} ${f(cy)} A${ro} ${ro} 0 1 1 ${f(cx - ro)} ${f(cy)} A${ro} ${ro} 0 1 1 ${f(cx + ro)} ${f(cy)}Z` +
    ` M${f(cx + ri)} ${f(cy)} A${ri} ${ri} 0 1 0 ${f(cx - ri)} ${f(cy)} A${ri} ${ri} 0 1 0 ${f(cx + ri)} ${f(cy)}Z`
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
