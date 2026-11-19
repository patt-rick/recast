/** The pad's state machine (spec §4.3). Pure: time comes in on events. */
import { OPEN_MS } from "./anim";
import type { Hit } from "./hitTest";
import { firstActive, firstPreset, isActive, nextActive, type Slot } from "./paging";

export const DWELL_MS = 600;
export const LEAVE_PX = 80;
export const LEAVE_MS = 600;
export const DONE_MS = 1200;
export const FAILED_MS = 2000;

export type Phase =
  | { name: "closed" }
  | { name: "opening"; at: number }
  | { name: "idle" }
  | { name: "targeting"; slot: number }
  | { name: "starting"; presetId: string }
  | { name: "running" }
  | { name: "done"; at: number }
  | { name: "failed"; at: number; message: string };

export interface PadState {
  phase: Phase;
  pages: Slot[][];
  page: number;
  /** When the page last changed (the symbols cross-fade). */
  pageAt: number;
  /** Keyboard focus on the current page. */
  focus: number | null;
  /** Pointer resting on More/Back since. */
  dwell: { slot: number; at: number } | null;
  /**
   * After a dwell turned the page, no dwell until the pointer leaves the slot it is over (`slot`,
   * learnt from the first pointer event; null until then). Pages hold different numbers
   * of slots, so the pointer that dwelt on More can land on the next page's Back and
   * would otherwise turn back, and forth, for as long as it stays.
   */
  dwellHold: { slot: number | null } | null;
  /** Pointer more than LEAVE_PX outside the pad since (drag mode). */
  outsideSince: number | null;
  /** The last input was a key: draw the focus outline. */
  keyboard: boolean;
  reducedMotion: boolean;
}

export type PadKey = "ArrowLeft" | "ArrowRight" | "Tab" | "ShiftTab" | "Enter" | "Escape";

export type PadEvent =
  | { type: "open"; pages: Slot[][]; at: number; reducedMotion: boolean }
  /** `dwell`: only drag-sourced positions arm the More/Back dwell (spec §4.2). */
  | { type: "pointer"; hit: Hit; at: number; dwell: boolean }
  | { type: "outside"; distance: number; at: number }
  /** A drop, or a click. `none` (outside the ring band) closes the pad; `gap` and `center` do nothing. */
  | { type: "drop"; hit: Hit; at: number }
  | { type: "key"; key: PadKey; at: number }
  | { type: "tick"; at: number }
  | { type: "started" }
  | { type: "startFailed"; message: string; at: number }
  | { type: "settled"; failed: string | null; at: number }
  | { type: "dismiss" };

export const closed: PadState = {
  phase: { name: "closed" },
  pages: [],
  page: 0,
  pageAt: 0,
  focus: null,
  dwell: null,
  dwellHold: null,
  outsideSince: null,
  keyboard: false,
  reducedMotion: false,
};

const interactive = (p: Phase) => p.name === "opening" || p.name === "idle" || p.name === "targeting";
const shut = (s: PadState): PadState => ({ ...closed, reducedMotion: s.reducedMotion });

/** The slots of the page on show. */
export const currentPage = (s: PadState): Slot[] => s.pages[s.page] ?? [];

/** The targeted slot, if any. */
export const target = (s: PadState): number | null => (s.phase.name === "targeting" ? s.phase.slot : null);

function turnPage(s: PadState, page: number, at: number): PadState {
  const p = Math.max(0, Math.min(s.pages.length - 1, page));
  return { ...s, page: p, pageAt: at, focus: firstPreset(s.pages[p] ?? []), dwell: null, dwellHold: null, phase: { name: "idle" } };
}

function activate(s: PadState, i: number | null, at: number): PadState {
  if (i == null) return s;
  const slot = currentPage(s)[i];
  switch (slot?.kind) {
    case "preset":
      return { ...s, dwell: null, phase: { name: "starting", presetId: slot.preset.id } };
    case "more":
      return turnPage(s, s.page + 1, at);
    case "back":
      return turnPage(s, s.page - 1, at);
    default:
      return s;
  }
}

export function reduce(s: PadState, e: PadEvent): PadState {
  if (e.type === "open") {
    return {
      ...closed,
      pages: e.pages,
      focus: firstActive(e.pages[0] ?? []),
      reducedMotion: e.reducedMotion,
      pageAt: e.at,
      phase: e.reducedMotion ? { name: "idle" } : { name: "opening", at: e.at },
    };
  }
  if (s.phase.name === "closed") return s;
  switch (e.type) {
    case "dismiss":
      return shut(s);
    case "tick": {
      const p = s.phase;
      if (p.name === "done" && e.at - p.at >= DONE_MS) return shut(s);
      if (p.name === "failed" && e.at - p.at >= FAILED_MS) return shut(s);
      if (!interactive(p)) return s;
      if (s.outsideSince != null && e.at - s.outsideSince >= LEAVE_MS) return shut(s);
      if (s.dwell && e.at - s.dwell.at >= DWELL_MS) {
        const next = activate({ ...s, dwell: null }, s.dwell.slot, e.at);
        // A dwell turned the page: hold further dwells until the pointer moves on.
        return next.page !== s.page ? { ...next, dwellHold: { slot: null } } : next;
      }
      if (p.name === "opening" && e.at - p.at >= OPEN_MS) return { ...s, phase: { name: "idle" } };
      return s;
    }
    case "pointer": {
      if (!interactive(s.phase)) return s;
      const slot = e.hit.kind === "slot" && isActive(currentPage(s)[e.hit.slot]) ? e.hit.slot : null;
      let hold = s.dwellHold;
      if (hold && (slot == null || (hold.slot != null && hold.slot !== slot))) hold = null;
      else if (hold && hold.slot == null) hold = { slot };
      if (slot == null) {
        return { ...s, dwell: null, dwellHold: hold, phase: s.phase.name === "targeting" ? { name: "idle" } : s.phase };
      }
      const kind = currentPage(s)[slot].kind;
      const pages = kind === "more" || kind === "back";
      const dwell = pages && e.dwell && !hold ? (s.dwell?.slot === slot ? s.dwell : { slot, at: e.at }) : null;
      return { ...s, dwell, dwellHold: hold, focus: slot, keyboard: false, phase: { name: "targeting", slot } };
    }
    case "outside":
      if (!interactive(s.phase)) return s;
    {
      const next = e.distance > LEAVE_PX ? (s.outsideSince ?? e.at) : null;
      return next === s.outsideSince ? s : { ...s, outsideSince: next };
    }
    case "drop":
      if (!interactive(s.phase)) return s;
      if (e.hit.kind === "center" || e.hit.kind === "gap") return s;
      if (e.hit.kind === "none") return shut(s);
      return activate(s, e.hit.slot, e.at);
    case "key": {
      if (e.key === "Escape") return shut(s);
      if (!interactive(s.phase)) return s;
      const page = currentPage(s);
      if (e.key === "Enter") return activate(s, s.focus, e.at);
      const dir = e.key === "ArrowRight" || e.key === "Tab" ? 1 : -1;
      return { ...s, keyboard: true, dwell: null, focus: nextActive(page, s.focus, dir) };
    }
    case "started":
      return s.phase.name === "starting" ? { ...s, phase: { name: "running" } } : s;
    case "startFailed":
      return s.phase.name === "starting" ? { ...s, phase: { name: "failed", at: e.at, message: e.message } } : s;
    case "settled":
      if (s.phase.name !== "running") return s;
      return { ...s, phase: e.failed == null ? { name: "done", at: e.at } : { name: "failed", at: e.at, message: e.failed } };
  }
}
