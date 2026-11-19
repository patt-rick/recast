import { formatBytes } from "../shared/format";
import type { PadModel } from "../shared/types";

export function formatDuration(s: number): string {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/**
 * Centre-disc text: name, meta (e.g. "MOV · 1.2 GB · 2:14") and an explanation when nothing applies.
 * `waiting`: a drag-mode pad opened with no files yet (spec A2).
 */
export function discText(model: PadModel, waiting = false): { title: string; meta: string; note: string | null } {
  if (waiting) return { title: "Drop files here", meta: "", note: null };
  const { files } = model;
  const one = files.length === 1 ? files[0] : null;
  const title = one ? one.name : files.length === 0 ? "Nothing to convert" : `${files.length} files`;
  const base = one
    ? [
        one.format.toUpperCase(),
        formatBytes(one.size),
        one.duration_s != null ? formatDuration(one.duration_s) : null,
        one.pages ? `${one.pages} pages` : null,
      ].filter(Boolean).join(" · ")
    : files.length > 0
      ? `${formatBytes(files.reduce((a, f) => a + f.size, 0))} in total`
      : "";
  const skipped = model.skipped > 0 ? `${model.skipped} skipped` : "";
  const meta = [base, skipped].filter(Boolean).join(" · ");
  const note = model.presets.length > 0
    ? null
    : files.length === 0
      ? "Folders aren't supported yet. Drop the files inside instead."
      : model.open_in_recast
        ? "These files are different kinds. Drop each kind separately."
        : "Recast has no conversions for this file.";
  return { title, meta, note };
}
