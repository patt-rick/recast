import { describe, expect, it, vi } from "vitest";
import { createRouter, type DragEv } from "./dragRouter";

describe("drag router", () => {
  it("sends events to the active section's handler only", () => {
    const r = createRouter();
    const a = vi.fn(), t = vi.fn();
    r.subscribe("activity", a);
    const off = r.subscribe("tools", t);
    const drop: DragEv = { type: "drop", paths: ["C:\\a.pdf"] };
    r.route("tools", drop);
    expect(t).toHaveBeenCalledWith(drop);
    expect(a).not.toHaveBeenCalled();
    off();
    r.route("tools", drop);
    expect(t).toHaveBeenCalledTimes(1);
  });

  it("drops anywhere but Tools go to Activity", () => {
    expect(createRouter().targetFor("library")).toBe("activity");
    expect(createRouter().targetFor("settings")).toBe("activity");
    expect(createRouter().targetFor("tools")).toBe("tools");
  });

  it("a stale unsubscribe doesn't remove a newer handler (StrictMode re-subscribe)", () => {
    const r = createRouter();
    const first = vi.fn(), second = vi.fn();
    const offFirst = r.subscribe("activity", first);
    r.subscribe("activity", second);
    offFirst();
    r.route("activity", { type: "leave" });
    expect(second).toHaveBeenCalledTimes(1);
  });
});
