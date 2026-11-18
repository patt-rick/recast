import type { JobEvent, JobRecord } from "../shared/types";
import { applyEvent, batchSuffix, empty, remove, seed, split, withRecord } from "./jobs";

const ev = (over: Partial<JobEvent>): JobEvent => ({
  job_id: "j1", batch_id: "b", batch_size: 1, preset_id: "video-mp3", preset_label: "MP3", file: "holiday.mov",
  status: "queued", pct: null, eta_s: null, out_path: null, error: null, ...over,
});

const rec = (over: Partial<JobRecord>): JobRecord => ({
  id: "old", preset_id: "image-png", preset_label: "PNG", inputs: ["C:\\p\\a.jpg", "C:\\p\\b.jpg"], output: null,
  status: "done", error_code: null, error_detail: null, stderr_tail: null, started_at: 1, ended_at: 2, temp_path: null, ...over,
});

test("seed builds views from history", () => {
  const s = seed(empty, [rec({})]);
  expect(s.order).toEqual(["old"]);
  expect(s.byId.old.file).toBe("a.jpg +1");
  expect(s.byId.old.presetLabel).toBe("PNG");
});

test("events add new jobs on top and update existing ones", () => {
  let s = seed(empty, [rec({})]);
  s = applyEvent(s, ev({}));
  expect(s.order).toEqual(["j1", "old"]);
  s = applyEvent(s, ev({ status: "running", pct: 0.5, eta_s: 10 }));
  expect(s.byId.j1.pct).toBe(0.5);
  s = applyEvent(s, ev({ status: "done", pct: 1, out_path: "C:\\v\\holiday.mp3" }));
  expect(s.order).toEqual(["j1", "old"]);
  expect(s.byId.j1.outPath).toBe("C:\\v\\holiday.mp3");
});

test("seed merges: applied events win, history-only rows are appended in order", () => {
  let s = applyEvent(empty, ev({ job_id: "live", status: "running", pct: 0.4 }));
  s = seed(s, [rec({ id: "live", status: "queued" }), rec({ id: "h1" }), rec({ id: "h2" })]);
  expect(s.order).toEqual(["live", "h1", "h2"]);
  expect(s.byId.live.status).toBe("running");
  expect(s.byId.live.pct).toBe(0.4);
  expect(s.byId.h2.status).toBe("done");
});

test("a batch lists in drop order, below newer batches and above older jobs", () => {
  let s = seed(empty, [rec({ id: "old" })]);
  s = applyEvent(s, ev({ job_id: "a", batch_id: "b1", batch_size: 3, file: "a.mov" }));
  s = applyEvent(s, ev({ job_id: "b", batch_id: "b1", batch_size: 3, file: "b.mov" }));
  s = applyEvent(s, ev({ job_id: "c", batch_id: "b1", batch_size: 3, file: "c.mov" }));
  expect(s.order).toEqual(["a", "b", "c", "old"]);
  s = applyEvent(s, ev({ job_id: "x", batch_id: "b2", file: "x.mov" }));
  s = applyEvent(s, ev({ job_id: "y", batch_id: "b2", batch_size: 2, file: "y.mov" }));
  expect(s.order).toEqual(["x", "y", "a", "b", "c", "old"]);
});

test("batch suffix shows only for active jobs in a batch of 2 or more", () => {
  let s = applyEvent(empty, ev({ job_id: "a", batch_size: 3, status: "running" }));
  s = applyEvent(s, ev({ job_id: "b", batch_size: 1, status: "running" }));
  s = applyEvent(s, ev({ job_id: "c", batch_size: 3, status: "done" }));
  s = seed(s, [rec({ id: "h" })]);
  expect(batchSuffix(s.byId.a)).toBe(" · batch of 3");
  expect(batchSuffix(s.byId.b)).toBe("");
  expect(batchSuffix(s.byId.c)).toBe("");
  expect(batchSuffix(s.byId.h)).toBe("");
});

test("split separates active jobs from history", () => {
  let s = applyEvent(empty, ev({ job_id: "a", status: "running" }));
  s = applyEvent(s, ev({ job_id: "b", status: "failed", error: { code: "ENGINE_FAILED", message: "x" } }));
  const { active, history } = split(s);
  expect(active.map((j) => j.id)).toEqual(["a"]);
  expect(history.map((j) => j.id)).toEqual(["b"]);
});

test("withRecord adds the stderr tail; remove drops a job", () => {
  let s = applyEvent(empty, ev({ status: "failed", error: { code: "ENGINE_FAILED", message: "x" } }));
  s = withRecord(s, rec({ id: "j1", status: "failed", stderr_tail: "boom" }));
  expect(s.byId.j1.stderrTail).toBe("boom");
  s = remove(s, "j1");
  expect(s.order).toEqual([]);
});
