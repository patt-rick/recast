import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DeepPartial, EngineEvent, EngineRow, JobEvent, JobOptions, JobRecord, Notice, PadModel, PadOpen, Settings } from "./types";

export const JOB_EVENTS = ["job://queued", "job://progress", "job://done", "job://failed", "job://cancelled"] as const;
export const ENGINE_EVENTS = ["engine://progress", "engine://done", "engine://failed"] as const;

export const ipc = {
  inspect: (paths: string[]) => invoke<PadModel>("inspect", { paths }),
  startJobs: (presetId: string, paths: string[], options?: JobOptions) =>
    invoke<string[]>("start_jobs", { presetId, paths, options: options ?? null }),
  /** Thumbnail bytes (PNG, or the image file itself); empty = show the type glyph. */
  thumbnail: (path: string) => invoke<ArrayBuffer>("thumbnail", { path }),
  /** Pad window only: the screen under the pad (u32 w, u32 h, RGBA). */
  captureBackdrop: () => invoke<ArrayBuffer>("capture_backdrop"),
  padReady: () => invoke<void>("pad_ready"),
  padShow: (generation: number) => invoke<void>("pad_show", { generation }),
  padClose: (generation: number) => invoke<void>("pad_close", { generation }),
  padTrackCursor: (on: boolean, generation: number) =>
    invoke<void>("pad_track_cursor", { on, generation }),
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
  /** settings.json couldn't be read at startup: nothing can be saved. */
  settingsReadOnly: () => invoke<boolean>("settings_read_only"),
  /** First run: pulse the edge handle while the welcome points at it. */
  highlightHandle: (on: boolean) => invoke<void>("highlight_handle", { on }),
  /** The pad's "Open in Recast": Activity lists these files grouped by kind. */
  openInRecast: (paths: string[]) => invoke<void>("open_in_recast", { paths }),
};

async function listenAll<T>(names: readonly string[], cb: (payload: T) => void): Promise<UnlistenFn> {
  const offs = await Promise.all(names.map((n) => listen<T>(n, (e) => cb(e.payload))));
  return () => offs.forEach((off) => off());
}

export const onJobEvent = (cb: (e: JobEvent) => void) => listenAll(JOB_EVENTS, cb);
export const onEngineEvent = (cb: (e: EngineEvent) => void) => listenAll(ENGINE_EVENTS, cb);
export type NavigatePayload = { section: "activity" | "library"; job_id: string | null };
/** A notification button or click asked the companion to go somewhere. */
export const onNavigate = (cb: (p: NavigatePayload) => void) =>
  listen<NavigatePayload>("companion://navigate", (e) => cb(e.payload));
/** The pad asked the companion to list these files grouped by kind. */
export const onOpenFiles = (cb: (paths: string[]) => void) =>
  listen<{ paths: string[] }>("companion://open-files", (e) => cb(e.payload.paths));
export const onPadOpen = (cb: (p: PadOpen) => void) => listen<PadOpen>("pad://open", (e) => cb(e.payload));
/** Cursor relative to the pad window, logical px (only while tracking). */
export const onPadCursor = (cb: (p: { x: number; y: number }) => void) =>
  listen<{ x: number; y: number }>("pad://cursor", (e) => cb(e.payload));
/** The window was asked to close (Alt+F4 etc.); the pad hides instead. */
export const onPadDismiss = (cb: () => void) => listen("pad://dismiss", () => cb());
