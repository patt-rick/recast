import type { PresetView } from "../shared/types";
import { N } from "./geometry";

export type Slot = { kind: "preset"; preset: PresetView } | { kind: "more" } | { kind: "back" } | { kind: "empty" };

const fill = (slots: Slot[]): Slot[] => [...slots, ...Array.from({ length: N - slots.length }, (): Slot => ({ kind: "empty" }))];
const preset = (p: PresetView): Slot => ({ kind: "preset", preset: p });

/** Presets → pages of exactly six slots (spec §4.2, deviation 7). */
export function paginate(presets: PresetView[]): Slot[][] {
  if (presets.length <= N) return [fill(presets.map(preset))];
  const pages: Slot[][] = [[...presets.slice(0, N - 1).map(preset), { kind: "more" }]];
  let rest = presets.slice(N - 1);
  while (rest.length > 0) {
    if (rest.length <= N - 1) {
      pages.push(fill([{ kind: "back" }, ...rest.map(preset)]));
      rest = [];
    } else {
      pages.push([{ kind: "back" }, ...rest.slice(0, N - 2).map(preset), { kind: "more" }]);
      rest = rest.slice(N - 2);
    }
  }
  return pages;
}

export const isActive = (s: Slot | undefined) => s != null && s.kind !== "empty";

export function firstActive(page: Slot[]): number | null {
  const i = page.findIndex(isActive);
  return i < 0 ? null : i;
}

/** Next targetable slot clockwise (dir 1) or anticlockwise (−1), wrapping. */
export function nextActive(page: Slot[], from: number | null, dir: 1 | -1): number | null {
  if (from == null) return firstActive(page);
  for (let k = 1; k <= N; k++) {
    const i = (((from + dir * k) % N) + N) % N;
    if (isActive(page[i])) return i;
  }
  return null;
}
