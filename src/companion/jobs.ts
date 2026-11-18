import { baseName } from "../shared/format";
import type { JobError, JobEvent, JobRecord, JobStatus } from "../shared/types";

export interface JobView {
  id: string;
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

export function seed(records: JobRecord[]): JobsState {
  return { order: records.map((r) => r.id), byId: Object.fromEntries(records.map((r) => [r.id, fromRecord(r)])) };
}

export function applyEvent(s: JobsState, e: JobEvent): JobsState {
  const prev = s.byId[e.job_id];
  const next: JobView = {
    id: e.job_id,
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
  return { order: prev ? s.order : [e.job_id, ...s.order], byId: { ...s.byId, [e.job_id]: next } };
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

export function split(s: JobsState): { active: JobView[]; history: JobView[] } {
  const all = s.order.map((id) => s.byId[id]);
  return { active: all.filter((j) => ACTIVE.includes(j.status)), history: all.filter((j) => !ACTIVE.includes(j.status)) };
}
