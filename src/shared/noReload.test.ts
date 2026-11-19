import { describe, expect, it } from "vitest";
import { isReloadKey } from "./noReload";

const k = (key: string, over: Partial<{ ctrlKey: boolean; metaKey: boolean }> = {}) => ({ key, ctrlKey: false, metaKey: false, ...over });

describe("isReloadKey", () => {
  it("catches F5, Ctrl+R, Cmd+R and Ctrl+Shift+R", () => {
    expect(isReloadKey(k("F5"))).toBe(true);
    expect(isReloadKey(k("r", { ctrlKey: true }))).toBe(true);
    expect(isReloadKey(k("R", { ctrlKey: true }))).toBe(true);
    expect(isReloadKey(k("r", { metaKey: true }))).toBe(true);
  });

  it("leaves everything else alone", () => {
    expect(isReloadKey(k("r"))).toBe(false);
    expect(isReloadKey(k("F4"))).toBe(false);
    expect(isReloadKey(k("a", { ctrlKey: true }))).toBe(false);
  });
});
