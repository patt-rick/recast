/** Drag-mode leave-to-dismiss helpers (spec §4.3). Pure. */
import { SIZE } from "./geometry";

/** Distance in logical px from the pad's square (0 when the point is inside it). */
export function outsideDistance(p: { x: number; y: number }): number {
  const dx = Math.max(0, -p.x, p.x - SIZE);
  const dy = Math.max(0, -p.y, p.y - SIZE);
  return Math.hypot(dx, dy);
}

/**
 * Leaving only dismisses once the cursor has been inside the pad at least once (or a drag
 * entered it), so a pad opened at the tray, away from the cursor, doesn't close at once.
 */
export function armedAfter(armed: boolean, p: { x: number; y: number }): boolean {
  return armed || outsideDistance(p) === 0;
}
