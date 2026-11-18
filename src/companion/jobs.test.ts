import type { JobEvent, JobRecord } from "../shared/types";
import { applyEvent, empty, remove, seed, split, withRecord } from "./jobs";

const ev = (over: Partial<JobEvent>): JobEvent => ({
  job_id: "j1", batch_id: "b", batch_size: 1, preset_id: "video-mp3", preset_label: "MP3", file: "holiday.mov",
  status: "queued", pct: null, eta_s: null, out_path: null, error: null, ...over,
});

const rec = (over: Partial<JobRecord>): JobRecord => ({
  id: "old", preset_id: "image-png", preset_label: "PNG", inputs: ["C:\\p\\a.jpg", "C:\\p\\b.jpg"], output: null,
  status: "done", error_code: null, error_detail: null, stderr_tail: null, started_at: 1, ended_at: 2, temp_path: null, ...over,
});

test("seed builds views from history", () => {
  const s = seed([rec({})]);
  expect(s.order).toEqual(["old"]);
  expect(s.byId.old.file).toBe("a.jpg +1");
  expect(s.byId.old.presetLabel).toBe("PNG");
});

test("events add new jobs on top and update existing ones", () => {
  let s = seed([rec({})]);
  s = applyEvent(s, ev({}));
  expect(s.order).toEqual(["j1", "old"]);
  s = applyEvent(s, ev({ status: "running", pct: 0.5, eta_s: 10 }));
  expect(s.byId.j1.pct).toBe(0.5);
  s = applyEvent(s, ev({ status: "done", pct: 1, out_path: "C:\\v\\holiday.mp3" }));
  expect(s.order).toEqual(["j1", "old"]);
  expect(s.byId.j1.outPath).toBe("C:\\v\\holiday.mp3");
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
