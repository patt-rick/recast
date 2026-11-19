import { describe, expect, it } from "vitest";
import { annulusPath, CX, CY, DISC_R, GAP, MAX_SLOTS, R, R0, R1, RC, SYMBOL, TH, inSector, rsector, slotAngle, step } from "./geometry";

const STEP = step(6);

const nums = (d: string) => (d.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);

describe("geometry constants (spec §4.4)", () => {
  it("matches the visual spec", () => {
    expect([R, TH, GAP, RC, MAX_SLOTS]).toEqual([94, 60, 4, 8, 6]);
    expect([R0, R1, DISC_R]).toEqual([64, 124, 59]);
    expect(slotAngle(0, 6)).toBeCloseTo(-Math.PI / 2);
    expect(slotAngle(3, 6)).toBeCloseTo(Math.PI / 2);
    expect(STEP).toBeCloseTo(Math.PI / 3);
  });
});

describe("rsector", () => {
  const d = rsector(CX, CY, R, TH, slotAngle(0, 6), STEP / 2, GAP, RC);

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
    const boundary = slotAngle(0, 6) - STEP / 2;
    const a = Math.atan2(y0 - CY, x0 - CX);
    const perp = R1 * Math.sin(a - boundary);
    expect(perp).toBeGreaterThan(GAP / 2);
  });
});

describe("inSector", () => {
  const at = (i: number, rad: number, da = 0) => {
    const a = slotAngle(i, 6) + da;
    return [CX + rad * Math.cos(a), CY + rad * Math.sin(a)] as const;
  };
  const inside = (x: number, y: number, i: number) => inSector(x, y, CX, CY, R, TH, slotAngle(i, 6), STEP / 2, GAP);

  it("contains the slot's middle and nothing in the disc, the gap or outside", () => {
    expect(inside(...at(0, R), 0)).toBe(true);
    expect(inside(...at(2, R), 2)).toBe(true);
    expect(inside(...at(0, R), 1)).toBe(false);
    expect(inside(...at(0, 60), 0)).toBe(false);
    expect(inside(...at(0, R1 + 2), 0)).toBe(false);
    expect(inside(...at(0, R, STEP / 2), 0)).toBe(false); // on the boundary ray: the gap
  });
});

describe("pages of fewer slots", () => {
  it("spreads n slots evenly from 12 o'clock", () => {
    expect(step(2)).toBeCloseTo(Math.PI);
    expect(slotAngle(1, 2)).toBeCloseTo(Math.PI / 2);
    expect(slotAngle(1, 3)).toBeCloseTo(-Math.PI / 2 + (2 * Math.PI) / 3);
    expect(Number.isFinite(slotAngle(0, 0))).toBe(true);
  });

  it("picks each arc's large-arc flag from the span it draws", () => {
    const flags = (hs: number) =>
      [...rsector(CX, CY, R, TH, slotAngle(0, 2), hs, GAP, RC).matchAll(/A[\d.]+ [\d.]+ 0 (\d) (\d)/g)].map((m) => m[1]);
    // A hovered half ring: the drawn arcs stay under π, both short.
    expect(flags(Math.PI / 2 + 0.03)).toEqual(["0", "0"]);
    // Wider than the corners can take back: the outer arc is long, the inner one not yet.
    expect(flags(Math.PI / 2 + 0.12)).toEqual(["1", "0"]);
    // Much wider: both long.
    expect(flags(Math.PI / 2 + 0.6)).toEqual(["1", "1"]);
  });

  it("draws one slot as a full ring with a hole (opposite windings)", () => {
    const d = annulusPath(CX, CY, R1, R0);
    expect(d.match(/A/g)?.length).toBe(4);
    const sweeps = [...d.matchAll(/A[\d.]+ [\d.]+ 0 1 (\d)/g)].map((m) => m[1]);
    expect(sweeps).toEqual(["1", "1", "0", "0"]);
  });
});

describe("tile contents", () => {
  // Corners of a tile's icon box, label box (at its widest) and badge, around the symbol centre.
  const boxes = (k: number) => {
    const { iconY, iconSize, labelY, labelMaxW, labelFont, badgeDx, badgeDy, badgeR } = SYMBOL;
    const pts: [number, number][] = [];
    const box = (x0: number, y0: number, x1: number, y1: number) => pts.push([x0, y0], [x1, y0], [x0, y1], [x1, y1]);
    box(-iconSize / 2, iconY - iconSize / 2, iconSize / 2, iconY + iconSize / 2);
    box(-labelMaxW / 2, labelY - labelFont / 2, labelMaxW / 2, labelY + labelFont / 2);
    for (let a = 0; a < 2 * Math.PI; a += Math.PI / 8) pts.push([badgeDx + badgeR * Math.cos(a), badgeDy + badgeR * Math.sin(a)]);
    return pts.map(([x, y]) => [x * k, y * k] as const);
  };

  it("keeps every icon, label and badge inside its tile, at rest and hovered, for six tiles", () => {
    for (let i = 0; i < 6; i++) {
      const ca = slotAngle(i, 6);
      for (const [k, th] of [[1, TH], [1.1, TH * 1.08]] as const) {
        const sx = CX + R * Math.cos(ca), sy = CY + R * Math.sin(ca);
        for (const [dx, dy] of boxes(k)) {
          const inside = inSector(sx + dx, sy + dy, CX, CY, R, th, ca, STEP / 2, GAP);
          expect(inside, `slot ${i} at scale ${k}: (${dx.toFixed(1)}, ${dy.toFixed(1)})`).toBe(true);
        }
      }
    }
  });
});
