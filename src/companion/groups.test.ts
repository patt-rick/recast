import { describe, expect, it } from "vitest";
import { groupByKind } from "./groups";
import type { FileInfo, Kind } from "../shared/types";

const f = (name: string, kind: Kind): FileInfo => ({
  path: `C:\\x\\${name}`, name, kind, ext: "", format: "", size: 1, duration_s: null, has_audio: null, pages: null,
});

describe("groupByKind", () => {
  it("groups in a fixed kind order, keeping drop order inside a group", () => {
    const g = groupByKind([f("b.png", "image"), f("r.pdf", "pdf"), f("a.png", "image"), f("v.mov", "video")]);
    expect(g.map((x) => x.kind)).toEqual(["video", "image", "pdf"]);
    expect(g[1]).toMatchObject({ label: "2 images", names: ["b.png", "a.png"], paths: ["C:\\x\\b.png", "C:\\x\\a.png"] });
    expect(g[2].label).toBe("1 PDF");
  });
  it("returns nothing for no files", () => {
    expect(groupByKind([])).toEqual([]);
  });
});
