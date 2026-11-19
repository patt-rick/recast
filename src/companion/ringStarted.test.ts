import { describe, expect, it } from "vitest";
import { shouldReportStarted } from "./ringStarted";

describe("shouldReportStarted", () => {
  it("waits for running, not starting", () => {
    expect(shouldReportStarted("starting", false)).toBe(false);
    expect(shouldReportStarted("running", false)).toBe(true);
  });
  it("only once", () => expect(shouldReportStarted("running", true)).toBe(false));
  it("a failed start never reports", () => expect(shouldReportStarted("failed", false)).toBe(false));
});
