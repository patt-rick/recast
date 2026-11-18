import { describe, expect, it } from "vitest";
import { approach, easeOutCubic, materialize, OPEN_MS } from "./anim";

describe("anim", () => {
  it("eases out and clamps", () => {
    expect(easeOutCubic(-1)).toBe(0);
    expect(easeOutCubic(1.5)).toBe(1);
    expect(easeOutCubic(0.5)).toBeCloseTo(0.875);
  });

  it("staggers the materialize by 55 ms per piece after a 120 ms delay", () => {
    expect(materialize(0, 0, false)).toBe(0);
    expect(materialize(120 + 520, 0, false)).toBe(1);
    expect(materialize(120 + 55 + 260, 1, false)).toBeCloseTo(0.875);
    expect(materialize(OPEN_MS, 6, false)).toBe(1);
    expect(materialize(0, 6, true)).toBe(1); // reduced motion: instant
  });

  it("springs at the same speed whatever the frame rate", () => {
    const at60 = approach(approach(0, 1, 16.667), 1, 16.667);
    const at30 = approach(0, 1, 33.333);
    expect(at60).toBeCloseTo(at30, 4);
    expect(approach(0, 1, 16.667)).toBeCloseTo(0.2, 3);
  });
});
