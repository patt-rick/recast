import type { PresetView } from "../shared/types";
import { MAX_SLOTS } from "./geometry";

export type Slot = { kind: "preset"; preset: PresetView } | { kind: "more" } | { kind: "back" };

const N = MAX_SLOTS;
const preset = (p: PresetView): Slot => ({ kind: "preset", preset: p });

/**
 * Presets → pages of at most six slots, each holding only real slots: the ring shows as
 * many tiles as the page has (spec §4.2). No presets → one page with no slots.
 */
export function paginate(presets: PresetView[]): Slot[][] {
  if (presets.length <= N) return [presets.map(preset)];
  const pages: Slot[][] = [[...presets.slice(0, N - 1).map(preset), { kind: "more" }]];
  let rest = presets.slice(N - 1);
  while (rest.length > 0) {
    if (rest.length <= N - 1) {
      pages.push([{ kind: "back" }, ...rest.map(preset)]);
      rest = [];
    } else {
      pages.push([{ kind: "back" }, ...rest.slice(0, N - 2).map(preset), { kind: "more" }]);
      rest = rest.slice(N - 2);
    }
  }
  return pages;
}

export const isActive = (s: Slot | undefined) => s != null;

export function firstActive(page: Slot[]): number | null {
  const i = page.findIndex(isActive);
  return i < 0 ? null : i;
}

export function lastActive(page: Slot[]): number | null {
  for (let i = page.length - 1; i >= 0; i--) if (isActive(page[i])) return i;
  return null;
}

/** First preset on the page, else the first active slot. */
export function firstPreset(page: Slot[]): number | null {
  const i = page.findIndex((s) => s.kind === "preset");
  return i < 0 ? firstActive(page) : i;
}

/** Next targetable slot clockwise (dir 1) or anticlockwise (−1), wrapping. */
export function nextActive(page: Slot[], from: number | null, dir: 1 | -1): number | null {
  if (from == null) return dir === 1 ? firstActive(page) : lastActive(page);
  const n = page.length;
  for (let k = 1; k <= n; k++) {
    const i = (((from + dir * k) % n) + n) % n;
    if (isActive(page[i])) return i;
  }
  return null;
}
