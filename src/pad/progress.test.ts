import { describe, expect, it } from "vitest";
import type { JobEvent, JobStatus } from "../shared/types";
import { emptyBatch, label, view, withEvent, withIds } from "./progress";

const ev = (job_id: string, status: JobStatus, pct: number | null = null, message?: string): JobEvent => ({
  job_id, batch_id: "b", batch_size: 2, preset_id: "video-mp3", preset_label: "MP3", file: `${job_id}.mov`,
  status, pct, eta_s: null, out_path: null, error: message ? { code: "ENGINE_FAILED", message } : null,
});

describe("batch progress", () => {
  it("treats known-but-empty ids as settled, unknown ids as not", () => {
    expect(view(withIds(emptyBatch(), [])).settled).toBe(true);
    expect(view(emptyBatch()).settled).toBe(false);
  });

  it("keeps events that arrive before start_jobs returns the ids", () => {
    let b = withEvent(emptyBatch(), ev("a", "queued"));
    b = withEvent(b, ev("other", "running", 0.5)); // a job from Activity: ignored once ids are known
    b = withIds(b, ["a"]);
    expect(view(b)).toMatchObject({ total: 1, finished: 0, settled: false, pct: null });
    b = withEvent(b, ev("a", "running", 0.42));
    expect(view(b).pct).toBeCloseTo(0.42);
    b = withEvent(b, ev("a", "done", null));
    expect(view(b)).toMatchObject({ settled: true, failed: null, pct: 1 });
  });

  it("aggregates a batch and reports the first failure", () => {
    let b = withIds(emptyBatch(), ["a", "b"]);
    b = withEvent(b, ev("a", "done"));
    b = withEvent(b, ev("b", "running", 0.5));
    expect(view(b)).toMatchObject({ total: 2, finished: 1, settled: false });
    expect(view(b).pct).toBeCloseTo(0.75);
    b = withEvent(b, ev("b", "failed", null, "Couldn't read the video."));
    expect(view(b)).toMatchObject({ settled: true, failed: "Couldn't read the video." });
  });

  it("counts skipped as success and cancelled as a failure", () => {
    const b = withEvent(withEvent(withIds(emptyBatch(), ["a", "b"]), ev("a", "skipped")), ev("b", "cancelled"));
    expect(view(b)).toMatchObject({ settled: true, failed: "Cancelled." });
  });

  it("labels the disc", () => {
    const one = withIds(emptyBatch(), ["a"]);
    expect(label(view(withEvent(one, ev("a", "running", 0.42))), null)).toBe("42%");
    expect(label(view(withEvent(one, ev("a", "running", null))), null)).toBe("Working…");
    expect(label(view(withEvent(one, ev("a", "waiting_engine"))), { engine_name: "FFmpeg", engine_size: 88_080_384 })).toBe("Getting FFmpeg · 84.0 MB");
    const two = withEvent(withEvent(withIds(emptyBatch(), ["a", "b"]), ev("a", "done")), ev("b", "running", 0.5));
    expect(label(view(two), null)).toBe("1 of 2 · 75%");
  });
});
