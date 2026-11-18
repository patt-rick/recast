import { describe, expect, it } from "vitest";
import { armedAfter, outsideDistance } from "./cursor";
import { SIZE } from "./geometry";

describe("pad cursor helpers", () => {
  it("measures distance from the pad square", () => {
    expect(outsideDistance({ x: 10, y: 10 })).toBe(0);
    expect(outsideDistance({ x: -30, y: 10 })).toBe(30);
    expect(outsideDistance({ x: SIZE + 3, y: SIZE + 4 })).toBe(5);
  });

  it("arms once the cursor has been inside, and stays armed", () => {
    expect(armedAfter(false, { x: -200, y: 0 })).toBe(false);
    expect(armedAfter(false, { x: 100, y: 100 })).toBe(true);
    expect(armedAfter(true, { x: -200, y: 0 })).toBe(true);
  });
});
