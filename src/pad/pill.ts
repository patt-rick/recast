/** Where the hover pill sits: in the band above or below the ring, never over it. */
import { CX, CY, LIFT, R1, SIZE, slotAngle } from "./geometry";

const X_OFFSET = 80;
/** Clear of a lifted tile. */
const CLEAR = R1 + LIFT + 3;
const MARGIN = 4;

/**
 * Slot `slot` of `n`: slots in the upper half (above 3 and 9 o'clock) put the pill above the
 * ring, the rest below; slots well to one side shift it that way. `y` is the pill's edge that
 * faces the ring (its bottom when `upper`, its top otherwise), so a taller pill grows away
 * from the ring. For six slots: 0, 1, 5 on top; 1, 2 right; 4, 5 left.
 */
export function pillAnchor(slot: number, n: number): { x: number; y: number; upper: boolean } {
  if (n <= 0) return { x: CX, y: CY - CLEAR, upper: true };
  const a = slotAngle(slot, n);
  const upper = Math.sin(a) < -1e-9;
  const c = Math.cos(a);
  const dx = c > 0.3 ? X_OFFSET : c < -0.3 ? -X_OFFSET : 0;
  return { x: CX + dx, y: upper ? CY - CLEAR : CY + CLEAR, upper };
}

/**
 * The pill's top edge: grown away from the ring from `anchor.y`, but kept inside
 * [4, SIZE - 4] (a tall pill then overlaps the ring band rather than leave the window).
 */
export function pillTop(anchor: { y: number; upper: boolean }, height: number): number {
  const top = anchor.upper ? anchor.y - height : anchor.y;
  return Math.max(MARGIN, Math.min(SIZE - MARGIN - height, top));
}

/** Keep a pill of `width` (centred on x) inside [4, SIZE - 4]. */
export function clampPillX(x: number, width: number): number {
  const half = width / 2;
  const lo = MARGIN + half;
  const hi = SIZE - MARGIN - half;
  if (lo > hi) return SIZE / 2;
  return Math.min(hi, Math.max(lo, x));
}
