import { describe, expect, it } from "vitest";
import { OPEN_IN_RECAST_ID, withOpenInRecast } from "./openInRecast";
import type { PadModel } from "../shared/types";

const m = (over: Partial<PadModel>): PadModel => ({ files: [], presets: [], open_in_recast: false, ...over });

describe("withOpenInRecast", () => {
  it("adds the one segment only when the files are mixed and nothing covers them all", () => {
    expect(withOpenInRecast(m({ open_in_recast: true })).presets.map((p) => p.id)).toEqual([OPEN_IN_RECAST_ID]);
    expect(withOpenInRecast(m({ open_in_recast: false })).presets).toEqual([]);
  });
});
