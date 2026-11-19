import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { OPEN_MS } from "./anim";
import { paginate } from "./paging";
import { closed, DONE_MS, DWELL_MS, FAILED_MS, LEAVE_MS, reduce, type PadEvent, type PadState } from "./states";

const p = (id: string): PresetView => ({
  id, label: id, description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: { state: "installed", version: "1", update_available: false }, min_files: 1, engine_size: 1, warning: null,
});
const pages = paginate(Array.from({ length: 8 }, (_, i) => p(`p${i + 1}`)));
const run = (events: PadEvent[], from: PadState = closed) => events.reduce(reduce, from);
const open = (at = 0, reducedMotion = false): PadEvent => ({ type: "open", pages, at, reducedMotion });
const slot = (s: number) => ({ kind: "slot" as const, slot: s });

describe("pad states (§4.3)", () => {
  it("opens, materializes, then idles", () => {
    const s = run([open(0)]);
    expect(s.phase).toEqual({ name: "opening", at: 0 });
    expect(s.focus).toBe(0);
    expect(run([open(0), { type: "tick", at: OPEN_MS - 1 }]).phase.name).toBe("opening");
    expect(run([open(0), { type: "tick", at: OPEN_MS }]).phase.name).toBe("idle");
    expect(run([open(0, true)]).phase.name).toBe("idle"); // reduced motion opens instantly
  });

  it("targets the slot under the pointer and lets go off the ring", () => {
    const s = run([open(), { type: "pointer", hit: slot(2), at: 5, dwell: false }]);
    expect(s.phase).toEqual({ name: "targeting", slot: 2 });
    expect(s.focus).toBe(2);
    expect(run([{ type: "pointer", hit: { kind: "none" }, at: 6, dwell: false }], s).phase.name).toBe("idle");
    expect(run([{ type: "pointer", hit: { kind: "center" }, at: 6, dwell: false }], s).phase.name).toBe("idle");
  });

  it("runs the preset dropped on, ignores the disc, closes on a drop outside", () => {
    expect(run([open(), { type: "drop", hit: slot(1), at: 9 }]).phase).toEqual({ name: "starting", presetId: "p2" });
    expect(run([open(), { type: "drop", hit: { kind: "center" }, at: 9 }]).phase.name).toBe("opening");
    expect(run([open(), { type: "drop", hit: { kind: "none" }, at: 9 }]).phase.name).toBe("closed");
    // Near misses (ring gaps, the band around the disc) are no-ops.
    expect(run([open(), { type: "drop", hit: { kind: "gap" }, at: 9 }]).phase.name).toBe("opening");
    expect(run([open(), { type: "pointer", hit: slot(2), at: 5, dwell: false }, { type: "pointer", hit: { kind: "gap" }, at: 6, dwell: false }]).phase.name).toBe("idle");
  });

  it("after a page turn, dwells only once the pointer has moved off the slot it landed on", () => {
    // 7 presets: page 2 is Back + 1, so Back's half ring covers where More was on page 1.
    const seven = paginate(Array.from({ length: 7 }, (_, i) => p(`p${i + 1}`)));
    const opened = run([{ type: "open", pages: seven, at: 0, reducedMotion: true }]);
    const turned = run([
      { type: "pointer", hit: slot(5), at: 100, dwell: true },
      { type: "tick", at: 100 + DWELL_MS },
    ], opened);
    expect(turned.page).toBe(1);
    // The same pointer is now over Back (slot 0): no dwell, however long it stays.
    const resting = run([
      { type: "pointer", hit: slot(0), at: 800, dwell: true },
      { type: "pointer", hit: slot(0), at: 900, dwell: true },
      { type: "tick", at: 5000 },
    ], turned);
    expect(resting.page).toBe(1);
    expect(resting.dwell).toBeNull();
    // Off it and back on: the dwell works again.
    const again = run([
      { type: "pointer", hit: slot(1), at: 5100, dwell: true },
      { type: "pointer", hit: slot(0), at: 5200, dwell: true },
      { type: "tick", at: 5200 + DWELL_MS },
    ], resting);
    expect(again.page).toBe(0);
  });

  it("dwell arms only for drag-sourced pointers", () => {
    const hover = run([open(), { type: "pointer", hit: slot(5), at: 100, dwell: false }]);
    expect(hover.dwell).toBeNull();
    expect(run([{ type: "tick", at: 5000 }], hover).page).toBe(0);
    const drag = run([open(), { type: "pointer", hit: slot(5), at: 100, dwell: true }]);
    expect(drag.dwell).toEqual({ slot: 5, at: 100 });
  });

  it("ignores outside and pointer while running, empty drops, early settles", () => {
    const running = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "started" }]);
    expect(run([{ type: "outside", distance: 200, at: 5 }, { type: "pointer", hit: slot(2), at: 6, dwell: true }], running)).toEqual(running);
    const three = paginate([p("a"), p("b"), p("c")]);
    const idle = run([{ type: "open", pages: three, at: 0, reducedMotion: true }, { type: "drop", hit: slot(4), at: 3 }]);
    expect(idle.phase.name).toBe("idle");
    expect(run([{ type: "settled", failed: null, at: 4 }], idle).phase.name).toBe("idle");
    const starting = run([{ type: "drop", hit: slot(0), at: 4 }], idle);
    expect(run([{ type: "settled", failed: null, at: 5 }], starting).phase.name).toBe("starting");
  });

  it("a pointer during opening targets", () => {
    const s = run([open(0), { type: "pointer", hit: slot(1), at: 10, dwell: false }]);
    expect(s.phase).toEqual({ name: "targeting", slot: 1 });
  });

  it("pages with More/Back on click or a 600 ms dwell", () => {
    const more = run([open(), { type: "drop", hit: slot(5), at: 10 }]);
    expect(more.page).toBe(1);
    expect(more.pageAt).toBe(10);
    expect(more.focus).toBe(1); // first preset, not Back
    const back = run([{ type: "drop", hit: slot(0), at: 20 }], more);
    expect(back.page).toBe(0);
    const hover = run([open(), { type: "pointer", hit: slot(5), at: 100, dwell: true }, { type: "pointer", hit: slot(5), at: 400, dwell: true }]);
    expect(run([{ type: "tick", at: 100 + DWELL_MS - 1 }], hover).page).toBe(0);
    expect(run([{ type: "tick", at: 100 + DWELL_MS }], hover).page).toBe(1);
    // Moving off More cancels the dwell.
    const off = run([{ type: "pointer", hit: slot(4), at: 300, dwell: true }, { type: "tick", at: 2000 }], hover);
    expect(off.page).toBe(0);
  });

  it("drives the ring from the keyboard", () => {
    const s = run([open(0, true), { type: "key", key: "ArrowRight", at: 1 }, { type: "key", key: "Tab", at: 2 }]);
    expect(s.focus).toBe(2);
    expect(run([{ type: "key", key: "ShiftTab", at: 3 }], s).focus).toBe(1);
    expect(run([{ type: "key", key: "ArrowLeft", at: 3 }, { type: "key", key: "ArrowLeft", at: 4 }, { type: "key", key: "ArrowLeft", at: 5 }], s).focus).toBe(5);
    expect(run([{ type: "key", key: "Enter", at: 4 }], s).phase).toEqual({ name: "starting", presetId: "p3" });
    const fresh = run([open(0, true)]);
    expect(run([{ type: "key", key: "ShiftTab", at: 1 }], { ...fresh, focus: null }).focus).toBe(5);
    expect(run([{ type: "key", key: "ArrowLeft", at: 1 }], { ...fresh, focus: null }).focus).toBe(5);
    const armed = run([{ type: "pointer", hit: slot(5), at: 1, dwell: true }], fresh);
    expect(run([{ type: "key", key: "Tab", at: 2 }], armed).dwell).toBeNull();
    expect(run([{ type: "key", key: "Escape", at: 4 }], s).phase.name).toBe("closed");
  });

  it("closes when a drag stays more than 80 px away for 600 ms", () => {
    const s = run([open(0, true), { type: "outside", distance: 120, at: 1000 }]);
    expect(s.outsideSince).toBe(1000);
    expect(run([{ type: "tick", at: 1000 + LEAVE_MS - 1 }], s).phase.name).toBe("idle");
    expect(run([{ type: "tick", at: 1000 + LEAVE_MS }], s).phase.name).toBe("closed");
    const back = run([{ type: "outside", distance: 40, at: 1200 }, { type: "tick", at: 3000 }], s);
    expect(back.phase.name).toBe("idle");
    expect(run([{ type: "outside", distance: 80, at: 1000 }], run([open(0, true)])).outsideSince).toBeNull();
  });

  it("runs, then shows done for 1.2 s or failed for 2 s, then closes", () => {
    const running = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "started" }]);
    expect(running.phase.name).toBe("running");
    // While running, pointer and drops do nothing; Esc dismisses (the job keeps going).
    expect(run([{ type: "drop", hit: slot(2), at: 5 }], running).phase.name).toBe("running");
    expect(run([{ type: "key", key: "Escape", at: 5 }], running).phase.name).toBe("closed");
    const done = run([{ type: "settled", failed: null, at: 100 }], running);
    expect(done.phase).toEqual({ name: "done", at: 100 });
    expect(run([{ type: "tick", at: 100 + DONE_MS }], done).phase.name).toBe("closed");
    const failed = run([{ type: "settled", failed: "The converter reported an error", at: 100 }], running);
    expect(failed.phase).toEqual({ name: "failed", at: 100, message: "The converter reported an error" });
    expect(run([{ type: "tick", at: 100 + FAILED_MS - 1 }], failed).phase.name).toBe("failed");
    expect(run([{ type: "tick", at: 100 + FAILED_MS }], failed).phase.name).toBe("closed");
    const refused = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "startFailed", message: "No", at: 2 }]);
    expect(refused.phase).toEqual({ name: "failed", at: 2, message: "No" });
  });

  it("a new trigger while running reopens with the new files", () => {
    const running = run([open(0, true), { type: "drop", hit: slot(0), at: 1 }, { type: "started" }]);
    expect(run([open(50)], running).phase).toEqual({ name: "opening", at: 50 });
  });

  it("ignores everything but open while closed", () => {
    expect(run([{ type: "drop", hit: slot(0), at: 1 }, { type: "key", key: "Enter", at: 1 }, { type: "tick", at: 9 }])).toBe(closed);
  });
});
