import { describe, expect, it } from "vitest";
import { CX, CY, DISC_R, GAP, N, R, R0, R1, RC, STEP, TH, inSector, rsector, slotAngle } from "./geometry";

const nums = (d: string) => (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);

describe("geometry constants (spec §4.4)", () => {
  it("matches the visual spec", () => {
    expect([R, TH, GAP, RC, N]).toEqual([136, 68, 9, 17, 6]);
    expect([R0, R1, DISC_R]).toEqual([102, 170, 88]);
    expect(slotAngle(0)).toBeCloseTo(-Math.PI / 2);
    expect(slotAngle(3)).toBeCloseTo(Math.PI / 2);
    expect(STEP).toBeCloseTo(Math.PI / 3);
  });
});

describe("rsector", () => {
  const d = rsector(CX, CY, R, TH, slotAngle(0), STEP / 2, GAP, RC);

  it("is one closed path of 2 arcs, 4 rounded corners and 2 sides", () => {
    expect(d.startsWith("M")).toBe(true);
    expect(d.endsWith("Z")).toBe(true);
    expect(d.match(/A/g)?.length).toBe(2);
    expect(d.match(/Q/g)?.length).toBe(4);
    expect(d.match(/L/g)?.length).toBe(2);
  });

  it("starts on the outer radius and is symmetric about the slot's axis", () => {
    const n = nums(d);
    const [x0, y0] = n;
    expect(Math.hypot(x0 - CX, y0 - CY)).toBeCloseTo(R1, 1);
    // The outer arc's end point (after "A ro ro 0 lg 1") mirrors the start across x = CX for slot 0.
    const [x1, y1] = n.slice(2 + 5, 2 + 7);
    expect(x0 + x1).toBeCloseTo(2 * CX, 1);
    expect(y0).toBeCloseTo(y1, 1);
  });

  it("leaves the gap: the outer edge stops gap/2 short of the boundary ray", () => {
    const [x0, y0] = nums(d);
    const boundary = slotAngle(0) - STEP / 2;
    const a = Math.atan2(y0 - CY, x0 - CX);
    const perp = R1 * Math.sin(a - boundary);
    expect(perp).toBeGreaterThan(GAP / 2);
  });
});

describe("inSector", () => {
  const at = (i: number, rad: number, da = 0) => {
    const a = slotAngle(i) + da;
    return [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;
  };
  const inside = (x: number, y: number, i: number) => inSector(x, y, CX, CY, R, TH, slotAngle(i), STEP / 2, GAP);

  it("contains the slot's middle and nothing in the disc, the gap or outside", () => {
    expect(inside(...at(0, R), 0)).toBe(true);
    expect(inside(...at(2, R), 2)).toBe(true);
    expect(inside(...at(0, R), 1)).toBe(false);
    expect(inside(...at(0, 60), 0)).toBe(false);
    expect(inside(...at(0, R1 + 2), 0)).toBe(false);
    expect(inside(...at(0, R, STEP / 2), 0)).toBe(false); // on the boundary ray: the gap
  });
});
