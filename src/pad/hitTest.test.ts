import { describe, expect, it } from "vitest";
import { CX, CY, R, R1, slotAngle, STEP } from "./geometry";
import { BAND_OUT, hitTest } from "./hitTest";

const polar = (rad: number, a: number) => [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;

describe("hitTest", () => {
  it("finds the centre disc, each slot, and nothing elsewhere", () => {
    expect(hitTest(CX, CY, null)).toEqual({ kind: "center" });
    for (let i = 0; i < 6; i++) expect(hitTest(...polar(R, slotAngle(i)), null)).toEqual({ kind: "slot", slot: i });
    expect(hitTest(...polar(91, slotAngle(0)), null)).toEqual({ kind: "none" }); // between disc and ring
    expect(hitTest(...polar(R1 + 20, slotAngle(1)), null)).toEqual({ kind: "slot", slot: 1 }); // generous outside
    expect(hitTest(...polar(BAND_OUT + 1, slotAngle(1)), null)).toEqual({ kind: "none" });
    expect(hitTest(2, 2, null)).toEqual({ kind: "none" });
  });

  it("treats the gaps as nothing unless the pointer is leaving one of their slots", () => {
    const boundary = slotAngle(1) + STEP / 2; // between slots 1 and 2 (3 o'clock)
    const p = polar(R, boundary);
    expect(hitTest(...p, null)).toEqual({ kind: "none" });
    expect(hitTest(...p, 1)).toEqual({ kind: "slot", slot: 1 });
    expect(hitTest(...p, 2)).toEqual({ kind: "slot", slot: 2 });
    expect(hitTest(...p, 4)).toEqual({ kind: "none" });
    // Slot 0 and slot 5 share the boundary at 11 o'clock (wrap-around).
    const wrap = polar(R, slotAngle(0) - STEP / 2);
    expect(hitTest(...wrap, 5)).toEqual({ kind: "slot", slot: 5 });
    expect(hitTest(...wrap, 0)).toEqual({ kind: "slot", slot: 0 });
  });
});
