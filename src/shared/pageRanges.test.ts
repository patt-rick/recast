import { describe, expect, it } from "vitest";
import { parseRanges } from "./pageRanges";

const err = (s: string, pages: number | null = null) => {
  const r = parseRanges(s, pages);
  return r.ok ? null : r.error;
};

describe("parseRanges (mirror of presets/ranges.rs)", () => {
  it("parses singles, spans and open ends", () => {
    expect(parseRanges(" 1-3, 5 ,8- ", null)).toEqual({
      ok: true,
      ranges: [{ start: 1, end: 3 }, { start: 5, end: 5 }, { start: 8, end: null }],
    });
    expect(parseRanges("2,,4", null)).toEqual({ ok: true, ranges: [{ start: 2, end: 2 }, { start: 4, end: 4 }] });
    expect(parseRanges("1-3,1-3", null)).toEqual({ ok: true, ranges: [{ start: 1, end: 3 }] });
  });

  it("rejects bad input with the same messages as Rust", () => {
    expect(err("")).toBe("Enter pages, e.g. 1-3, 5, 8-");
    expect(err(" , ")).toBe("Enter pages, e.g. 1-3, 5, 8-");
    expect(err("0")).toBe("Pages start at 1.");
    expect(err("3-1")).toBe('"3-1" runs backwards.');
    expect(err("a")).toBe('"a" isn\'t a page number or range.');
    expect(err("-4")).toBe('"-4" isn\'t a page number or range.');
    expect(err("1-2-3")).toBe('"1-2-3" isn\'t a page number or range.');
    expect(err("+5")).toBe('"+5" isn\'t a page number or range.');
    expect(err("99999999999")).toBe('"99999999999" isn\'t a page number or range.');
  });

  it("checks against the page count", () => {
    expect(err("1-10", 10)).toBeNull();
    expect(err("9-11", 10)).toBe('This PDF has 10 pages; "9-11" goes past the end.');
    expect(err("11-", 10)).not.toBeNull();
    expect(err("10-", 10)).toBeNull();
  });

  it("caps the number of ranges", () => {
    const many = Array.from({ length: 201 }, (_, i) => String(i + 1)).join(",");
    expect(err(many)).toBe("At most 200 ranges at a time.");
  });
});
