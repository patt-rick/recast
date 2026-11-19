import { describe, expect, it } from "vitest";
import { planDrop } from "./dropPlan";

const s = { waiting: false, paths: ["a.pdf"] };

describe("planDrop", () => {
  it("keeps the pad open when a slot, the centre or a gap is hit", () => {
    expect(planDrop(s, ["a.pdf"], { kind: "slot", slot: 1 })).toBe("run-and-keep");
    expect(planDrop(s, ["a.pdf"], { kind: "center" })).toBe("run-and-keep");
    expect(planDrop(s, ["a.pdf"], { kind: "gap" })).toBe("run-and-keep");
  });
  it("closes when the drop is outside the ring", () => {
    expect(planDrop(s, ["a.pdf"], { kind: "none" })).toBe("run-and-close");
  });
  it("reopens for different files, and for a waiting pad", () => {
    expect(planDrop(s, ["b.pdf"], { kind: "slot", slot: 0 })).toBe("reopen");
    expect(planDrop({ waiting: true, paths: [] }, ["b.pdf"], { kind: "center" })).toBe("reopen");
  });
  it("ignores an empty drop on a different session", () => {
    expect(planDrop(s, [], { kind: "center" })).toBe("ignore");
  });
});
