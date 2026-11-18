/** Page ranges for Split PDF. Mirrors src-tauri/src/presets/ranges.rs: keep both identical. */
export interface PageRange {
  start: number;
  /** null = to the last page. */
  end: number | null;
}

export type RangesResult = { ok: true; ranges: PageRange[] } | { ok: false; error: string };

export const MAX_RANGES = 200;
const U32_MAX = 4294967295;

function num(s: string, whole: string): number | string {
  const bad = `"${whole}" isn't a page number or range.`;
  if (!/^\d+$/.test(s)) return bad;
  const n = Number(s);
  if (n > U32_MAX) return bad;
  if (n === 0) return "Pages start at 1.";
  return n;
}

export function parseRanges(s: string, pages: number | null): RangesResult {
  const out: PageRange[] = [];
  for (const part of s.split(",")) {
    const p = part.replace(/\s+/g, "");
    if (p === "") continue;
    const dash = p.indexOf("-");
    let r: PageRange;
    if (dash < 0) {
      const n = num(p, p);
      if (typeof n === "string") return { ok: false, error: n };
      r = { start: n, end: n };
    } else {
      const a = num(p.slice(0, dash), p);
      if (typeof a === "string") return { ok: false, error: a };
      const rest = p.slice(dash + 1);
      if (rest === "") {
        r = { start: a, end: null };
      } else {
        const b = num(rest, p);
        if (typeof b === "string") return { ok: false, error: b };
        if (a > b) return { ok: false, error: `"${p}" runs backwards.` };
        r = { start: a, end: b };
      }
    }
    if (pages != null) {
      const last = r.end ?? r.start;
      if (r.start > pages || last > pages) {
        return { ok: false, error: `This PDF has ${pages} pages; "${p}" goes past the end.` };
      }
    }
    if (!out.some((x) => x.start === r.start && x.end === r.end)) {
      if (out.length === MAX_RANGES) return { ok: false, error: `At most ${MAX_RANGES} ranges at a time.` };
      out.push(r);
    }
  }
  if (out.length === 0) return { ok: false, error: "Enter pages, e.g. 1-3, 5, 8-" };
  return { ok: true, ranges: out };
}
