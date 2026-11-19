import { describe, expect, it } from "vitest";
import { clampPillX, pillAnchor } from "./pill";

describe("pillAnchor", () => {
  it("places all six slots above or below the ring", () => {
    expect(pillAnchor(0)).toEqual({ x: 220, y: 22 });
    expect(pillAnchor(1)).toEqual({ x: 325, y: 22 });
    expect(pillAnchor(2)).toEqual({ x: 325, y: 418 });
    expect(pillAnchor(3)).toEqual({ x: 220, y: 418 });
    expect(pillAnchor(4)).toEqual({ x: 115, y: 418 });
    expect(pillAnchor(5)).toEqual({ x: 115, y: 22 });
  });
});

describe("clampPillX", () => {
  it("leaves a centred pill alone", () => {
    expect(clampPillX(220, 200)).toBe(220);
  });
  it("keeps the right edge inside 436", () => {
    expect(clampPillX(325, 300)).toBe(286);
  });
  it("keeps the left edge inside 4", () => {
    expect(clampPillX(115, 300)).toBe(154);
  });
  it("centres a pill wider than the pad", () => {
    expect(clampPillX(325, 500)).toBe(220);
  });
});
