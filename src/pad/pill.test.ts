import { describe, expect, it } from "vitest";
import { clampPillX, pillAnchor, pillTop } from "./pill";

// Pad 368, centre 184; pills clear a lifted tile: 184 ∓ (124 + 9 + 3) = 48 / 320; sides ±80.
describe("pillAnchor", () => {
  it("places all six slots above or below the ring, by the edge facing it", () => {
    expect(pillAnchor(0, 6)).toEqual({ x: 184, y: 48, upper: true });
    expect(pillAnchor(1, 6)).toEqual({ x: 264, y: 48, upper: true });
    expect(pillAnchor(2, 6)).toEqual({ x: 264, y: 320, upper: false });
    expect(pillAnchor(3, 6)).toEqual({ x: 184, y: 320, upper: false });
    expect(pillAnchor(4, 6)).toEqual({ x: 104, y: 320, upper: false });
    expect(pillAnchor(5, 6)).toEqual({ x: 104, y: 48, upper: true });
  });
});

describe("pillAnchor for fewer slots", () => {
  it("puts top-half slots above the ring and the rest below", () => {
    expect(pillAnchor(0, 2)).toEqual({ x: 184, y: 48, upper: true });
    expect(pillAnchor(1, 2)).toEqual({ x: 184, y: 320, upper: false });
    expect(pillAnchor(0, 1)).toEqual({ x: 184, y: 48, upper: true });
    expect(pillAnchor(1, 3)).toEqual({ x: 264, y: 320, upper: false });
    expect(pillAnchor(2, 3)).toEqual({ x: 104, y: 320, upper: false });
    expect(pillAnchor(0, 0)).toEqual({ x: 184, y: 48, upper: true });
  });
});

describe("clampPillX", () => {
  it("leaves a centred pill alone", () => {
    expect(clampPillX(184, 200)).toBe(184);
  });
  it("keeps the right edge inside 364", () => {
    expect(clampPillX(264, 300)).toBe(214);
  });
  it("keeps the left edge inside 4", () => {
    expect(clampPillX(104, 300)).toBe(154);
  });
  it("centres a pill wider than the pad", () => {
    expect(clampPillX(264, 400)).toBe(184);
  });
});

describe("pillTop", () => {
  it("grows away from the ring", () => {
    expect(pillTop({ y: 48, upper: true }, 28)).toBe(20);
    expect(pillTop({ y: 320, upper: false }, 28)).toBe(320);
  });
  it("keeps a tall pill inside the window", () => {
    expect(pillTop({ y: 48, upper: true }, 58)).toBe(4);
    expect(pillTop({ y: 320, upper: false }, 58)).toBe(368 - 4 - 58);
  });
});
