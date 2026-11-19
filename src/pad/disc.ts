import type { PadModel } from "../shared/types";

export function formatDuration(s: number): string {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/**
 * Centre text (§4.2): the name (one file), "N files", or "Drop files here" while waiting
 * (spec A2); "N skipped" when some couldn't be read; and a note when nothing applies.
 * Type, size and length are not shown (the owner chose name only, 2026-11-19).
 */
export function discText(model: PadModel, waiting = false): { title: string; skipped: string | null; note: string | null } {
  if (waiting) return { title: "Drop files here", skipped: null, note: null };
  const { files } = model;
  const one = files.length === 1 ? files[0] : null;
  const title = one ? one.name : files.length === 0 ? "Nothing to convert" : `${files.length} files`;
  // On its own line, so a long name can't push it out of view.
  const skipped = model.skipped > 0 ? `${model.skipped} skipped` : null;
  const note = model.presets.length > 0
    ? null
    : files.length === 0
      ? "Folders aren't supported yet. Drop the files inside instead."
      : model.open_in_recast
        ? "These files are different kinds. Drop each kind separately."
        : "Recast has no conversions for this file.";
  return { title, skipped, note };
}
