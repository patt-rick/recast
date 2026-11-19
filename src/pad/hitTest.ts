import { angleDiff, CX, CY, DISC_R, EMPTY_DISC_R, GAP, R1, slotAngle, step } from "./geometry";

export type Hit = { kind: "slot"; slot: number } | { kind: "center" } | { kind: "gap" } | { kind: "none" };
/** gap: inside the ring band but over no slot (a click there is not a dismissal); none: outside the band. */

/** The ring answers from the disc's edge (5 px inside its glass) to well outside it. */
export const BAND_OUT = R1 + 34;

/**
 * Point (logical px in the pad) → what it is over, on a page of `n` slots. In a gap, the
 * slot the pointer is leaving stays targeted (no flicker); otherwise a gap is nothing.
 * With no slots there is no band: only the disc answers. One slot is the whole ring.
 */
export function hitTest(x: number, y: number, current: number | null, n: number): Hit {
  const dx = x - CX, dy = y - CY;
  const rad = Math.hypot(dx, dy);
  // With no tiles the disc is drawn larger, and all of it is the centre.
  if (rad <= (n <= 0 ? EMPTY_DISC_R : DISC_R)) return { kind: "center" };
  if (n <= 0 || rad > BAND_OUT) return { kind: "none" };
  if (n === 1) return { kind: "slot", slot: 0 };
  const st = step(n);
  const a = Math.atan2(dy, dx);
  const slot = (((Math.round((a + Math.PI / 2) / st) % n) + n) % n);
  const rel = angleDiff(a, slotAngle(slot, n));
  const side = rel >= 0 ? 1 : -1;
  const boundary = slotAngle(slot, n) + (side * st) / 2;
  if (rad * Math.abs(Math.sin(a - boundary)) < GAP / 2) {
    const neighbour = (slot + side + n) % n;
    return current === slot || current === neighbour ? { kind: "slot", slot: current } : { kind: "gap" };
  }
  return { kind: "slot", slot };
}
