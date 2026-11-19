import { describe, expect, it } from "vitest";
import type { FileInfo, PadModel } from "../shared/types";
import { discText, formatDuration } from "./disc";

const file = (over: Partial<FileInfo> = {}): FileInfo => ({
  path: "C:\\v\\holiday.mov", name: "holiday.mov", kind: "video", ext: "mov", format: "mov",
  size: 1288490189, duration_s: 134, has_audio: true, pages: null, ...over,
});
const model = (files: FileInfo[], presets = 1, open_in_recast = false, skipped = 0): PadModel => ({
  files, open_in_recast, skipped,
  presets: Array.from({ length: presets }, () => ({
    id: "x", label: "X", description: "x", icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
    engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
  })),
});

describe("disc text (§4.2)", () => {
  it("formats durations", () => {
    expect(formatDuration(134)).toBe("2:14");
    expect(formatDuration(5)).toBe("0:05");
    expect(formatDuration(3725)).toBe("1:02:05");
  });

  it("names one file with kind, size and length or pages", () => {
    expect(discText(model([file()]))).toEqual({ title: "holiday.mov", meta: "MOV · 1.2 GB · 2:14", note: null });
    expect(discText(model([file({ name: "a.pdf", kind: "pdf", format: "pdf", size: 2048, duration_s: null, pages: 3 })])).meta).toBe("PDF · 2.0 KB · 3 pages");
  });

  it("counts several files", () => {
    expect(discText(model([file(), file({ size: 10 })])).title).toBe("2 files");
  });

  it("explains an empty ring", () => {
    expect(discText(model([], 0)).note).toBe("Folders aren't supported yet. Drop the files inside instead.");
    expect(discText(model([file(), file({ kind: "pdf" })], 0, true)).note).toBe("These files are different kinds. Drop each kind separately.");
    expect(discText(model([file()], 0)).note).toBe("Recast has no conversions for this file.");
  });

  it("mentions files that couldn't be read", () => {
    expect(discText(model([file(), file({ size: 10 })], 1, false, 1)).meta).toBe("1.2 GB in total · 1 skipped");
    expect(discText(model([file()], 1, false, 2)).meta).toBe("MOV · 1.2 GB · 2:14 · 2 skipped");
    // Everything failed: the empty-model note stays, with the count beside it.
    const none = discText(model([], 0, false, 3));
    expect(none.meta).toBe("3 skipped");
    expect(none.note).toBe("Folders aren't supported yet. Drop the files inside instead.");
  });

  it("invites a drop while waiting for files", () => {
    expect(discText(model([], 0), true)).toEqual({ title: "Drop files here", meta: "", note: null });
  });
});
