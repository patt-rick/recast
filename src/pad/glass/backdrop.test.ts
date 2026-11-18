import { describe, expect, it } from "vitest";
import { decode } from "./backdrop";

describe("decode", () => {
  it("reads width, height and RGBA rows", () => {
    const buf = new Uint8Array([2, 0, 0, 0, 1, 0, 0, 0, 3, 2, 1, 255, 30, 20, 10, 255]).buffer;
    const px = decode(buf);
    expect([px.width, px.height]).toEqual([2, 1]);
    expect(Array.from(px.data)).toEqual([3, 2, 1, 255, 30, 20, 10, 255]);
  });

  it("refuses truncated data", () => {
    expect(() => decode(new Uint8Array([2, 0, 0, 0, 1, 0, 0, 0, 1]).buffer)).toThrow();
    expect(() => decode(new Uint8Array([1, 0]).buffer)).toThrow();
  });
});
