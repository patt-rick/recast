import { angleDiff, CX, CY, DISC_R, GAP, N, R0, R1, slotAngle, STEP } from "./geometry";

export type Hit = { kind: "slot"; slot: number } | { kind: "center" } | { kind: "gap" } | { kind: "none" };
/** gap: inside the ring band but over no slot (a click there is not a dismissal); none: outside the band. */

/** The ring answers a little inside and well outside its glass (the mockup's tolerance). */
export const BAND_IN = R0 - 8;
export const BAND_OUT = R1 + 34;

/**
 * Point (logical px in the pad) → what it is over. In a gap, the slot the pointer is
 * leaving stays targeted (no flicker); otherwise a gap is nothing.
 */
export function hitTest(x: number, y: number, current: number | null): Hit {
  const dx = x - CX, dy = y - CY;
  const rad = Math.hypot(dx, dy);
  if (rad <= DISC_R) return { kind: "center" };
  if (rad > BAND_OUT) return { kind: "none" };
  if (rad < BAND_IN) return { kind: "gap" };
  const a = Math.atan2(dy, dx);
  const slot = (((Math.round((a + Math.PI / 2) / STEP) % N) + N) % N);
  const rel = angleDiff(a, slotAngle(slot));
  const side = rel >= 0 ? 1 : -1;
  const boundary = slotAngle(slot) + (side * STEP) / 2;
  if (rad * Math.abs(Math.sin(a - boundary)) < GAP / 2) {
    const neighbour = (slot + side + N) % N;
    return current === slot || current === neighbour ? { kind: "slot", slot: current } : { kind: "gap" };
  }
  return { kind: "slot", slot };
}
