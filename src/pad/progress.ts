/** What the pad shows while its jobs run (spec §4.3 running state, batch indicator). */
import { formatBytes } from "../shared/format";
import type { JobEvent, JobStatus } from "../shared/types";

export interface BatchJob {
  status: JobStatus;
  pct: number | null;
  message: string | null;
}

export interface Batch {
  /** null until start_jobs returns. */
  ids: string[] | null;
  /** Latest event per job, including ones that came before the ids were known. */
  jobs: Record<string, BatchJob>;
}

export interface BatchView {
  total: number;
  finished: number;
  pct: number | null;
  waitingEngine: boolean;
  settled: boolean;
  /** The first failure's message, once settled with a failure. */
  failed: string | null;
}

const ENDED: JobStatus[] = ["done", "skipped", "failed", "cancelled", "interrupted"];
const BAD: JobStatus[] = ["failed", "cancelled", "interrupted"];

export const emptyBatch = (): Batch => ({ ids: null, jobs: {} });

export function withEvent(b: Batch, e: JobEvent): Batch {
  if (b.ids && !b.ids.includes(e.job_id)) return b;
  return { ...b, jobs: { ...b.jobs, [e.job_id]: { status: e.status, pct: e.pct, message: e.error?.message ?? null } } };
}

export function withIds(b: Batch, ids: string[]): Batch {
  const jobs: Record<string, BatchJob> = {};
  for (const id of ids) if (b.jobs[id]) jobs[id] = b.jobs[id];
  return { ids, jobs };
}

export function view(b: Batch): BatchView {
  const ids = b.ids ?? [];
  const list = ids.map((id) => b.jobs[id]);
  const finished = list.filter((j) => j && ENDED.includes(j.status)).length;
  // Known-but-empty ids (start_jobs created nothing) is settled: nothing will ever finish.
  // Unknown ids (null) are not: start_jobs is still in flight.
  const settled = b.ids !== null && finished === ids.length;
  const bad = list.find((j) => j && BAD.includes(j.status));
  let pct: number | null;
  if (ids.length === 0) pct = null;
  else if (ids.length === 1) {
    const j = list[0];
    pct = j && ENDED.includes(j.status) ? 1 : j?.status === "running" ? j.pct : null;
  } else {
    const running = list.reduce((a, j) => a + (j && j.status === "running" ? (j.pct ?? 0) : 0), 0);
    pct = (finished + running) / ids.length;
  }
  return {
    total: ids.length,
    finished,
    pct,
    waitingEngine: list.some((j) => j?.status === "waiting_engine"),
    settled,
    failed: settled && bad ? (bad.message ?? (bad.status === "cancelled" ? "Cancelled." : "Couldn't convert.")) : null,
  };
}

/** The disc's status line. `engine` describes the running preset's engine. */
export function label(v: BatchView, engine: { engine_name: string; engine_size: number } | null): string {
  if (v.waitingEngine && engine) return `Getting ${engine.engine_name} · ${formatBytes(engine.engine_size)}`;
  const pct = v.pct != null ? `${Math.round(v.pct * 100)}%` : null;
  if (v.total > 1) return [`${v.finished} of ${v.total}`, pct].filter(Boolean).join(" · ");
  return pct ?? "Working…";
}
