import { describe, expect, it } from "vitest";
import type { FileInfo } from "../shared/types";
import { accept, rejection } from "./toolList";

const f = (name: string, kind: FileInfo["kind"]): FileInfo => ({
  path: `C:\\d\\${name}`, name, kind, ext: name.split(".").pop()!, format: "x", size: 1, duration_s: null, has_audio: null, pages: kind === "pdf" ? 2 : null,
});
let n = 0;
const id = () => `id${n++}`;

describe("tool lists", () => {
  it("keeps the right kind in drop order and skips duplicates", () => {
    const first = accept([f("b.pdf", "pdf"), f("notes.docx", "text"), f("a.pdf", "pdf")], "pdf", [], id);
    expect(first.items.map((i) => i.name)).toEqual(["b.pdf", "a.pdf"]);
    expect(first.rejected).toEqual(["notes.docx"]);
    const again = accept([f("a.pdf", "pdf"), f("c.pdf", "pdf")], "pdf", first.items, id);
    expect(again.items.map((i) => i.name)).toEqual(["b.pdf", "a.pdf", "c.pdf"]);
    expect(again.items[0].pages).toBe(2);
  });

  it("explains what was left out", () => {
    expect(rejection(["notes.docx"], "pdf")).toBe("notes.docx isn't a PDF.");
    expect(rejection(["a.mov", "b.mov"], "image")).toBe("2 files aren't images.");
    expect(rejection([], "pdf")).toBeNull();
  });
});
