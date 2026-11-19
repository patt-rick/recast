/** Where the hover pill sits: in the band above or below the ring, never over it. */
import { CX, SIZE } from "./geometry";

const X_OFFSET = 105;
const MARGIN = 4;

export function pillAnchor(slot: number): { x: number; y: number } {
  const upper = slot === 0 || slot === 1 || slot === 5;
  const dx = slot === 1 || slot === 2 ? X_OFFSET : slot === 4 || slot === 5 ? -X_OFFSET : 0;
  return { x: CX + dx, y: upper ? 22 : 418 };
}

/** Keep a pill of `width` (centred on x) inside [4, SIZE - 4]. */
export function clampPillX(x: number, width: number): number {
  const half = width / 2;
  const lo = MARGIN + half;
  const hi = SIZE - MARGIN - half;
  if (lo > hi) return SIZE / 2;
  return Math.min(hi, Math.max(lo, x));
}
