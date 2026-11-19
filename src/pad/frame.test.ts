import { describe, expect, it } from "vitest";
import type { PresetView } from "../shared/types";
import { OPEN_MS } from "./anim";
import { buildFrame, restSprings, stepSprings, type FrameInput } from "./frame";
import { CX, R } from "./geometry";
import { paginate } from "./paging";

const p = (id: string, installed = true, warning: string | null = null): PresetView => ({
  id, label: id.toUpperCase(), description: id, icon: "film", engine: "ffmpeg", engine_name: "FFmpeg",
  engine_status: installed ? { state: "installed", version: "1", update_available: false } : { state: "not_installed" },
  min_files: 1, engine_size: 1, warning,
});
const page = paginate([p("a"), p("b", false), p("c", true, "Big"), p("d"), p("e"), p("f")])[0];
const base = (over: Partial<FrameInput> = {}): FrameInput => ({
  now: OPEN_MS, openedAt: 0, page, pageAt: -1000, target: null, focus: null, showFocus: false,
  springs: restSprings(), lum: [0.1, 0.1, 0.9, 0.1, 0.1, 0.1], discLum: 0.1, solid: null,
  reducedMotion: false, pointer: null, retract: 0, progress: null, ...over,
});

describe("buildFrame", () => {
  it("shows a fully materialized ring at rest", () => {
    const f = buildFrame(base());
    expect(f.slots).toHaveLength(6);
    expect(f.slots[0].mag).toBeCloseTo(1.1);
    expect(f.slots[0].opacity).toBeCloseTo(1);
    expect(f.slots[0].symbolColor).toBe("#fff");
    expect(f.slots[2].light).toBe(true);
    expect(f.slots[2].symbolColor).toBe("#1d1d1f");
    expect(f.slots[1].badge).toBe("download");
    expect(f.slots[2].badge).toBe("warning");
  });

  it("draws only the slots a page has", () => {
    expect(buildFrame(base({ page: page.slice(0, 2), lum: [0.1, 0.1] })).slots).toHaveLength(2);
    expect(buildFrame(base({ page: [], lum: [] })).slots).toHaveLength(0);
  });

  it("draws one slot as the whole ring: magnified about the centre, never lifted", () => {
    const springs = restSprings();
    springs[0] = { h: 1, off: 9 };
    const s = buildFrame(base({ page: page.slice(0, 1), lum: [0.1], target: 0, springs })).slots[0];
    expect([s.ox, s.oy]).toEqual([CX, CX]);
    expect(s.path.match(/A/g)?.length).toBe(4); // the annulus
    expect(Math.hypot(s.sx - CX, s.sy - CX)).toBeCloseTo(R, 0); // symbol at 12 o'clock, no lift
    expect(s.sy).toBeLessThan(CX);
  });

  it("magnifies wide slots less", () => {
    const three = buildFrame(base({ page: page.slice(0, 3), lum: [0.1, 0.1, 0.1] })).slots[0];
    expect(three.mag).toBeCloseTo(1.04);
  });

  it("starts the open animation small and unmagnified", () => {
    const f = buildFrame(base({ now: 0 }));
    expect(f.slots[0].opacity).toBe(0);
    expect(f.slots[0].mag).toBe(1);
  });

  it("lifts and tints the targeted slot, white symbols even over light", () => {
    const springs = restSprings();
    springs[2] = { h: 1, off: 9 };
    const f = buildFrame(base({ target: 2, springs }));
    const s = f.slots[2];
    expect(s.tint).toBe("rgba(10,132,255,0.800)");
    expect(s.symbolColor).toBe("#fff");
    expect(s.mag).toBeCloseTo(1.15);
    expect(Math.hypot(s.sx - CX, s.sy - CX)).toBeCloseTo(R + 9, 0);
  });

  it("reduced motion: tint only, no lift or scale", () => {
    const springs = restSprings();
    springs[0] = { h: 1, off: 9 };
    const s = buildFrame(base({ reducedMotion: true, target: 0, springs })).slots[0];
    expect(s.tint).toBe("rgba(10,132,255,0.450)");
    expect(s.symbolScale).toBe(1);
    expect(Math.hypot(s.sx - CX, s.sy - CX)).toBeCloseTo(R, 0);
  });

  it("uses the solid surface when there is no backdrop", () => {
    const f = buildFrame(base({ lum: null, discLum: null, solid: "light" }));
    expect(f.slots[0].light).toBe(true);
    expect(f.solid).toBe("light");
  });

  it("retracts the segments while running and draws a progress rim", () => {
    const f = buildFrame(base({ retract: 1, progress: { pct: 0.5, spin: 0 } }));
    expect(f.slots.every((s) => s.opacity === 0 && s.symbolAlpha === 0)).toBe(true);
    expect(f.ring).toEqual({ pct: 0.5, spin: 0 });
  });
});

describe("retract", () => {
  it("draws no slot below keep 0.2 and keeps the segment shape", () => {
    for (const retract of [0.81, 0.9, 1]) {
      const f = buildFrame(base({ retract }));
      expect(f.slots.every((s) => s.opacity === 0)).toBe(true);
    }
    expect(buildFrame(base({ retract: 0.79 })).slots[0].opacity).toBeGreaterThan(0);
  });

  it("moves radius toward the disc and fades, without shrinking or blur changes", () => {
    const rad = (r: number) => {
      const s = buildFrame(base({ retract: r })).slots[0];
      return { d: Math.hypot(s.sx - CX, s.sy - CX), o: s.opacity, b: s.blurMix, path: s.path };
    };
    const a = rad(0), b = rad(0.4), c = rad(0.7);
    expect(a.d).toBeCloseTo(R);
    expect(b.d).toBeLessThan(a.d);
    expect(c.d).toBeLessThan(b.d);
    expect(b.o).toBeCloseTo(0.6);
    expect(c.o).toBeCloseTo(0.3);
    expect(b.b).toBe(1);
  });
});

describe("springs", () => {
  it("approach the target slot and settle back", () => {
    let s = restSprings();
    for (let k = 0; k < 60; k++) s = stepSprings(s, 1, 16.667, false);
    expect(s[1].h).toBeCloseTo(1, 2);
    expect(s[1].off).toBeCloseTo(9, 1);
    expect(s[0].h).toBe(0);
    const reduced = stepSprings(restSprings(), 1, 16.667, true);
    expect(reduced[1].off).toBe(0);
  });
});
