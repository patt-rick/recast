/** Mixed kinds with no common preset (spec §5.6): one segment that opens Activity with the files grouped. */
import type { PadModel, PresetView } from "../shared/types";

export const OPEN_IN_RECAST_ID = "open-in-recast";

export const OPEN_IN_RECAST: PresetView = {
  id: OPEN_IN_RECAST_ID,
  label: "Open",
  description: "Open in Recast · convert each kind on its own",
  icon: "stack",
  engine: "",
  engine_name: "",
  engine_status: { state: "installed", version: "", update_available: false },
  min_files: 1,
  engine_size: 0,
  warning: null,
};

export function withOpenInRecast(model: PadModel): PadModel {
  return model.open_in_recast && model.presets.length === 0 ? { ...model, presets: [OPEN_IN_RECAST] } : model;
}
