import { describe, expect, it } from "vitest";
import { CX, CY, DISC_R, EMPTY_DISC_R, R, R1, slotAngle, step } from "./geometry";

const STEP = step(6);
import { BAND_OUT, hitTest } from "./hitTest";

const polar = (rad: number, a: number) => [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;

describe("hitTest", () => {
  it("finds the centre disc, each slot, and nothing elsewhere", () => {
    expect(hitTest(CX, CY, null, 6)).toEqual({ kind: "center" });
    for (let i = 0; i < 6; i++) expect(hitTest(...polar(R, slotAngle(i, 6)), null, 6)).toEqual({ kind: "slot", slot: i });
    expect(hitTest(...polar(DISC_R - 1, slotAngle(0, 6)), null, 6)).toEqual({ kind: "center" }); // the centre reaches almost to the ring
    expect(hitTest(...polar(DISC_R + 2, slotAngle(0, 6)), null, 6)).toEqual({ kind: "slot", slot: 0 }); // just inside the ring answers
    expect(hitTest(...polar(R1 + 20, slotAngle(1, 6)), null, 6)).toEqual({ kind: "slot", slot: 1 }); // generous outside
    expect(hitTest(...polar(BAND_OUT + 1, slotAngle(1, 6)), null, 6)).toEqual({ kind: "none" });
    expect(hitTest(2, 2, null, 6)).toEqual({ kind: "none" });
  });

  it("treats the gaps as gaps unless the pointer is leaving one of their slots", () => {
    const boundary = slotAngle(1, 6) + STEP / 2; // between slots 1 and 2 (3 o'clock)
    const p = polar(R, boundary);
    expect(hitTest(...p, null, 6)).toEqual({ kind: "gap" });
    expect(hitTest(...p, 1, 6)).toEqual({ kind: "slot", slot: 1 });
    expect(hitTest(...p, 2, 6)).toEqual({ kind: "slot", slot: 2 });
    expect(hitTest(...p, 4, 6)).toEqual({ kind: "gap" });
    // Slot 0 and slot 5 share the boundary at 11 o'clock (wrap-around).
    const wrap = polar(R, slotAngle(0, 6) - STEP / 2);
    expect(hitTest(...wrap, 5, 6)).toEqual({ kind: "slot", slot: 5 });
    expect(hitTest(...wrap, 0, 6)).toEqual({ kind: "slot", slot: 0 });
  });

  it("has no band without slots, and one slot is the whole ring", () => {
    expect(hitTest(...polar(R, slotAngle(2, 6)), null, 0)).toEqual({ kind: "none" });
    expect(hitTest(CX, CY, null, 0)).toEqual({ kind: "center" });
    // The whole drawn disc is the centre, so a click on its edge doesn't close the pad.
    expect(hitTest(...polar(EMPTY_DISC_R - 1, 1), null, 0)).toEqual({ kind: "center" });
    expect(hitTest(...polar(EMPTY_DISC_R + 2, 1), null, 0)).toEqual({ kind: "none" });
    for (const a of [0, 1, 2, 3, 4, 5]) expect(hitTest(...polar(R, a), null, 1)).toEqual({ kind: "slot", slot: 0 });
  });

  it("splits two slots into top and bottom halves", () => {
    expect(hitTest(...polar(R, -Math.PI / 2), null, 2)).toEqual({ kind: "slot", slot: 0 });
    expect(hitTest(...polar(R, Math.PI / 2), null, 2)).toEqual({ kind: "slot", slot: 1 });
    expect(hitTest(...polar(R, -2.6), null, 2)).toEqual({ kind: "slot", slot: 0 }); // 10 o'clock
    expect(hitTest(...polar(R, 0), null, 2)).toEqual({ kind: "gap" }); // 3 o'clock: the gap
  });
});
