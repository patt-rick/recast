import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { firstActive, nextActive, paginate } from "./paging";

const p = (id: string): PresetView => ({
  id, label: id, description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
});
const kinds = (page: ReturnType<typeof paginate>[number]) =>
  page.map((s) => (s.kind === "preset" ? s.preset.id : s.kind));
const many = (n: number) => Array.from({ length: n }, (_, i) => p(`p${i + 1}`));

describe("paginate", () => {
  it("always gives pages of six slots", () => {
    expect(kinds(paginate([])[0])).toEqual(["empty", "empty", "empty", "empty", "empty", "empty"]);
    expect(kinds(paginate(many(3))[0])).toEqual(["p1", "p2", "p3", "empty", "empty", "empty"]);
    expect(paginate(many(6)).map(kinds)).toEqual([["p1", "p2", "p3", "p4", "p5", "p6"]]);
  });

  it("puts More in slot 6 and Back in slot 1 when there are more than six", () => {
    expect(paginate(many(7)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "empty", "empty", "empty"],
    ]);
    expect(paginate(many(10)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "p8", "p9", "p10"],
    ]);
    expect(paginate(many(11)).map(kinds)).toEqual([
      ["p1", "p2", "p3", "p4", "p5", "more"],
      ["back", "p6", "p7", "p8", "p9", "more"],
      ["back", "p10", "p11", "empty", "empty", "empty"],
    ]);
  });
});

describe("keyboard order", () => {
  const page = paginate(many(3))[0];
  it("skips empty slots and wraps", () => {
    expect(firstActive(page)).toBe(0);
    expect(nextActive(page, 2, 1)).toBe(0);
    expect(nextActive(page, 0, -1)).toBe(2);
    expect(nextActive(page, null, 1)).toBe(0);
    expect(nextActive(page, null, -1)).toBe(2);
    expect(firstActive(paginate([])[0])).toBeNull();
  });
});
