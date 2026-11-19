export type Kind = "video" | "audio" | "image" | "pdf" | "text" | "unknown";

export interface FileInfo {
  path: string;
  name: string;
  kind: Kind;
  ext: string;
  format: string;
  size: number;
  duration_s: number | null;
  has_audio: boolean | null;
  pages: number | null;
}

export type EngineStatus =
  | { state: "not_installed" }
  | { state: "installing"; pct: number }
  | { state: "installed"; version: string; update_available: boolean }
  | { state: "damaged" };

export interface PresetView {
  id: string;
  label: string;
  description: string;
  icon: string;
  engine: string;
  engine_name: string;
  engine_status: EngineStatus;
  min_files: number;
  /** Engine download size in bytes (for "Getting FFmpeg · 84 MB"). */
  engine_size: number;
  /** A caution shown in the ring, e.g. GIF from a long video. */
  warning: string | null;
}

export interface PadModel {
  files: FileInfo[];
  presets: PresetView[];
  open_in_recast: boolean;
  /** Files that couldn't be read and were left out. */
  skipped: number;
}

export type JobStatus = "queued" | "waiting_engine" | "running" | "done" | "skipped" | "failed" | "cancelled" | "interrupted";

export interface JobError {
  code: string;
  message: string;
}

export interface JobEvent {
  job_id: string;
  batch_id: string;
  batch_size: number;
  preset_id: string;
  preset_label: string;
  file: string;
  status: JobStatus;
  pct: number | null;
  eta_s: number | null;
  out_path: string | null;
  error: JobError | null;
}

export interface JobRecord {
  /** Insertion order; the `list_jobs` paging cursor. */
  seq: number;
  id: string;
  preset_id: string;
  preset_label: string;
  inputs: string[];
  output: string | null;
  status: JobStatus;
  error_code: string | null;
  error_detail: string | null;
  stderr_tail: string | null;
  started_at: number;
  ended_at: number | null;
  temp_path: string | null;
  /** JSON JobOptions, or null. */
  options: string | null;
}

export interface EngineRow {
  id: string;
  name: string;
  version: string;
  size: number;
  licence: string;
  homepage: string;
  source_url: string | null;
  status: EngineStatus;
  disk_bytes: number;
}

export type EngineEvent =
  | { kind: "progress"; id: string; pct: number }
  | { kind: "done"; id: string }
  | { kind: "failed"; id: string; error: string };

export interface Notice {
  name: string;
  licence: string;
  homepage: string;
  source_url: string | null;
  bundled: boolean;
}

export interface Settings {
  version: number;
  triggers: { edge_handle: boolean; edge: "left" | "right" | "top" | "bottom"; edge_pos: number; desk_pad: boolean; shake: boolean; desk_x: number; desk_y: number };
  shell: { explorer_entry: boolean };
  output: { location: "next_to_original" | "folder"; folder: string; fallback_folder: string };
  quality: { mp3_kbps: number; aac_kbps: number; jpg_q: number; webp_q: number; avif_q: number };
  gif: { fps: number; width: number };
  shrink: { target_mb: number; image_px: number };
  jobs: { concurrency: number; stall_timeout_s: number; no_progress_cap_min: number };
  app: { start_with_windows: boolean; first_run_done: boolean };
}

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };

export type PadMode = "click" | "drag";

export interface PadOpen {
  paths: string[];
  mode: PadMode;
  reduced_transparency: boolean;
  /** Increments on every open; pad_show / pad_close are no-ops for a stale one. */
  generation: number;
  /** A shake probe: invisible until a file drag enters it. */
  probe: boolean;
  /** Leave-to-dismiss is armed from the start (the open came from an anchored trigger). */
  armed: boolean;
}

export interface JobOptions {
  /** Page ranges for "pdf-extract", e.g. "1-3, 5, 8-". */
  ranges?: string;
}
