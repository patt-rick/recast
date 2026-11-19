import { describe, expect, it } from "vitest";
import { shouldHighlight, showWelcome } from "./welcomeState";

describe("showWelcome", () => {
  const base = { loaded: true, firstRunDone: false, closed: false, readOnly: false as boolean | null };
  it("shows on a first run with writable settings", () => expect(showWelcome(base)).toBe(true));
  it("stays away until the read-only answer is in", () => expect(showWelcome({ ...base, readOnly: null })).toBe(false));
  it("stays away when settings can't be saved", () => expect(showWelcome({ ...base, readOnly: true })).toBe(false));
  it("stays away once done, closed or before settings load", () => {
    expect(showWelcome({ ...base, firstRunDone: true })).toBe(false);
    expect(showWelcome({ ...base, closed: true })).toBe(false);
    expect(showWelcome({ ...base, loaded: false })).toBe(false);
  });
});

describe("shouldHighlight", () => {
  it("only on step 0 while the window is visible and focused", () => {
    expect(shouldHighlight({ step: 0, visible: true, focused: true })).toBe(true);
    expect(shouldHighlight({ step: 1, visible: true, focused: true })).toBe(false);
    expect(shouldHighlight({ step: 0, visible: false, focused: true })).toBe(false);
    expect(shouldHighlight({ step: 0, visible: true, focused: false })).toBe(false);
  });
});
