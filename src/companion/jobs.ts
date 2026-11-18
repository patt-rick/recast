import { baseName } from "../shared/format";
import type { JobError, JobEvent, JobRecord, JobStatus } from "../shared/types";

export interface JobView {
  id: string;
  /** Jobs dropped together; unknown for rows loaded from history. */
  batchId: string | null;
  batchSize: number | null;
  presetId: string;
  presetLabel: string;
  file: string;
  status: JobStatus;
  pct: number | null;
  etaS: number | null;
  outPath: string | null;
  error: JobError | null;
  stderrTail: string | null;
}

export interface JobsState {
  order: string[];
  byId: Record<string, JobView>;
}

export const empty: JobsState = { order: [], byId: {} };
export const ACTIVE: JobStatus[] = ["queued", "waiting_engine", "running"];

export function fromRecord(r: JobRecord): JobView {
  const first = r.inputs[0] ? baseName(r.inputs[0]) : "";
  return {
    id: r.id,
    batchId: null,
    batchSize: null,
    presetId: r.preset_id,
    presetLabel: r.preset_label,
    file: r.inputs.length > 1 ? `${first} +${r.inputs.length - 1}` : first,
    status: r.status,
    pct: null,
    etaS: null,
    outPath: r.output,
    error: r.error_code ? { code: r.error_code, message: r.error_detail ?? "" } : null,
    stderrTail: r.stderr_tail,
  };
}

/** Merge history into what live events already built: events win, history-only rows follow in order. */
export function seed(s: JobsState, records: JobRecord[]): JobsState {
  const byId = { ...Object.fromEntries(records.map((r) => [r.id, fromRecord(r)])), ...s.byId };
  const extra = records.map((r) => r.id).filter((id) => !(id in s.byId));
  return { order: [...s.order, ...extra], byId };
}

/** New jobs go on top, but a batch lists in drop order: after the batch's other jobs. */
function insertNew(s: JobsState, e: JobEvent): string[] {
  let last = -1;
  s.order.forEach((id, i) => {
    if (s.byId[id].batchId === e.batch_id) last = i;
  });
  if (last < 0) return [e.job_id, ...s.order];
  return [...s.order.slice(0, last + 1), e.job_id, ...s.order.slice(last + 1)];
}

export function applyEvent(s: JobsState, e: JobEvent): JobsState {
  const prev = s.byId[e.job_id];
  const next: JobView = {
    id: e.job_id,
    batchId: e.batch_id,
    batchSize: e.batch_size,
    presetId: e.preset_id,
    presetLabel: e.preset_label,
    file: e.file,
    status: e.status,
    pct: e.pct ?? (ACTIVE.includes(e.status) ? prev?.pct ?? null : null),
    etaS: e.eta_s,
    outPath: e.out_path ?? prev?.outPath ?? null,
    error: e.error,
    stderrTail: prev?.stderrTail ?? null,
  };
  return { order: prev ? s.order : insertNew(s, e), byId: { ...s.byId, [e.job_id]: next } };
}

export function withRecord(s: JobsState, r: JobRecord): JobsState {
  const prev = s.byId[r.id];
  if (!prev) return s;
  return { ...s, byId: { ...s.byId, [r.id]: { ...prev, stderrTail: r.stderr_tail } } };
}

export function remove(s: JobsState, id: string): JobsState {
  const byId = { ...s.byId };
  delete byId[id];
  return { order: s.order.filter((x) => x !== id), byId };
}

/** " · batch of N" for an active job dropped with others; empty otherwise. */
export function batchSuffix(j: JobView): string {
  return ACTIVE.includes(j.status) && j.batchSize != null && j.batchSize > 1 ? ` · batch of ${j.batchSize}` : "";
}

export function split(s: JobsState): { active: JobView[]; history: JobView[] } {
  const all = s.order.map((id) => s.byId[id]);
  return { active: all.filter((j) => ACTIVE.includes(j.status)), history: all.filter((j) => !ACTIVE.includes(j.status)) };
}

/** Rows per `list_jobs` page (matches the backend). */
export const PAGE_SIZE = 50;

/** Append one more page of history without duplicating rows already shown; `more` is false once a page comes back short. */
export function appendHistory(s: JobsState, records: JobRecord[]): { state: JobsState; more: boolean } {
  return { state: seed(s, records), more: records.length >= PAGE_SIZE };
}
