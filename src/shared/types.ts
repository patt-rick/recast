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
}

export interface PadModel {
  files: FileInfo[];
  presets: PresetView[];
  open_in_recast: boolean;
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
  triggers: { edge_handle: boolean; edge: "left" | "right" | "top" | "bottom"; edge_pos: number; desk_pad: boolean; shake: boolean };
  shell: { explorer_entry: boolean };
  output: { location: "next_to_original" | "folder"; folder: string; fallback_folder: string };
  quality: { mp3_kbps: number; aac_kbps: number; jpg_q: number; webp_q: number; avif_q: number };
  gif: { fps: number; width: number };
  shrink: { target_mb: number; image_px: number };
  jobs: { concurrency: number; stall_timeout_s: number; no_progress_cap_min: number };
  app: { start_with_windows: boolean; first_run_done: boolean };
}

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };
