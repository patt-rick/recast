import { describe, expect, it } from "vitest";
import { enterAction } from "./enter";

const s = (over: Partial<{ generation: number; paths: string[]; probe: boolean }> = {}) => ({
  generation: 3,
  paths: [] as string[],
  probe: false,
  ...over,
});

describe("enterAction", () => {
  it("ignores an enter with no session, or while a newer open is still loading", () => {
    expect(enterAction(null, 3, ["a"])).toBe("ignore");
    expect(enterAction(s({ generation: 2 }), 3, ["a"])).toBe("ignore");
  });

  it("ignores a path-less drag (text, link) over a probe", () => {
    expect(enterAction(s({ probe: true }), 3, [])).toBe("ignore");
  });

  it("re-begins a probe, or any session, when the dragged files differ", () => {
    expect(enterAction(s({ probe: true }), 3, ["a"])).toBe("rebegin");
    expect(enterAction(s({ paths: ["a"] }), 3, ["b"])).toBe("rebegin");
  });

  it("just tracks when the files match, or the drag carries no paths", () => {
    expect(enterAction(s({ paths: ["a"] }), 3, ["a"])).toBe("track");
    expect(enterAction(s({ paths: ["a"] }), 3, [])).toBe("track");
  });
});
