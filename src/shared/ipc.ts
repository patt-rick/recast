import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DeepPartial, EngineEvent, EngineRow, JobEvent, JobRecord, Notice, PadModel, Settings } from "./types";

export const JOB_EVENTS = ["job://queued", "job://progress", "job://done", "job://failed", "job://cancelled"] as const;
export const ENGINE_EVENTS = ["engine://progress", "engine://done", "engine://failed"] as const;

export const ipc = {
  inspect: (paths: string[]) => invoke<PadModel>("inspect", { paths }),
  startJobs: (presetId: string, paths: string[]) => invoke<string[]>("start_jobs", { presetId, paths }),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  retryJob: (id: string, fallback = false) => invoke<string[]>("retry_job", { id, fallback }),
  /** Newest first; `beforeSeq` = the smallest `seq` already loaded (omit for the first page). */
  listJobs: (beforeSeq?: number) => invoke<JobRecord[]>("list_jobs", { beforeSeq: beforeSeq ?? null }),
  removeJob: (id: string) => invoke<void>("remove_job", { id }),
  enginesList: () => invoke<EngineRow[]>("engines_list"),
  engineInstall: (id: string) => invoke<void>("engine_install", { id }),
  engineRemove: (id: string) => invoke<void>("engine_remove", { id }),
  engineVerify: (id: string) => invoke<boolean>("engine_verify", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: DeepPartial<Settings>) => invoke<Settings>("set_settings", { patch }),
  /** Only a finished job's saved file can be shown or opened; the backend looks it up. */
  revealJob: (id: string) => invoke<void>("reveal_job", { id }),
  openJob: (id: string) => invoke<void>("open_job", { id }),
  /** Licence text of the notice at this index of `notices()`: the engine's own file, else the bundled text. */
  noticeText: (index: number, name: string) => invoke<string>("notice_text", { index, name }),
  notices: () => invoke<Notice[]>("notices"),
};

async function listenAll<T>(names: readonly string[], cb: (payload: T) => void): Promise<UnlistenFn> {
  const offs = await Promise.all(names.map((n) => listen<T>(n, (e) => cb(e.payload))));
  return () => offs.forEach((off) => off());
}

export const onJobEvent = (cb: (e: JobEvent) => void) => listenAll(JOB_EVENTS, cb);
export const onEngineEvent = (cb: (e: EngineEvent) => void) => listenAll(ENGINE_EVENTS, cb);
