import { describe, expect, it } from "vitest";
import { SIZE } from "./geometry";
import { discLuminance, isLight, luma, slotLuminance, type Pixels } from "./luminance";

function image(scale: number, color: (x: number, y: number) => number): Pixels {
  const w = SIZE * scale, h = SIZE * scale;
  const data = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const v = color(x / scale, y / scale);
      data.set([v, v, v, 255], (y * w + x) * 4);
    }
  return { data, width: w, height: h };
}

describe("luminance", () => {
  it("uses Rec. 709 luma on 0..1", () => {
    expect(luma(255, 255, 255)).toBeCloseTo(1);
    expect(luma(0, 0, 0)).toBe(0);
    expect(luma(0, 255, 0)).toBeCloseTo(0.7152);
  });

  it("averages what is under each slot, at any DPI", () => {
    for (const scale of [1, 1.5, 2]) {
      const leftWhite = image(scale, (x) => (x < SIZE / 2 ? 255 : 0));
      // Slots 4 and 5 are on the left (8 and 10 o'clock); 1 and 2 on the right.
      expect(isLight(slotLuminance(leftWhite, scale, 4))).toBe(true);
      expect(isLight(slotLuminance(leftWhite, scale, 5))).toBe(true);
      expect(isLight(slotLuminance(leftWhite, scale, 1))).toBe(false);
      expect(isLight(slotLuminance(leftWhite, scale, 2))).toBe(false);
    }
    expect(slotLuminance(image(1, () => 255), 1, 0)).toBeCloseTo(1);
    expect(discLuminance(image(1, () => 0), 1)).toBe(0);
  });

  it("switches at 0.6 exactly as the spec says (> 0.6 is light)", () => {
    expect(isLight(0.6)).toBe(false);
    expect(isLight(0.61)).toBe(true);
  });
});
