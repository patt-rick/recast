import { useEffect, useReducer, useState, type Dispatch } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ipc, onJobEvent } from "../shared/ipc";
import { formatBytes, formatEta } from "../shared/format";
import type { JobEvent, JobRecord, PadModel } from "../shared/types";
import { applyEvent, empty, remove, seed, split, withRecord, type JobsState, type JobView } from "./jobs";

type Action =
  | { type: "seed"; records: JobRecord[] }
  | { type: "event"; e: JobEvent }
  | { type: "record"; r: JobRecord }
  | { type: "remove"; id: string };

function reducer(s: JobsState, a: Action): JobsState {
  switch (a.type) {
    case "seed":
      return seed(a.records);
    case "event":
      return applyEvent(s, a.e);
    case "record":
      return withRecord(s, a.r);
    case "remove":
      return remove(s, a.id);
  }
}

interface Dropped {
  paths: string[];
  pad: PadModel;
}

export function Activity() {
  const [jobs, dispatch] = useReducer(reducer, empty);
  const [dropped, setDropped] = useState<Dropped | null>(null);
  const [hovering, setHovering] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc.listJobs(0).then((records) => dispatch({ type: "seed", records })).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    const off = onJobEvent((e) => {
      dispatch({ type: "event", e });
      if (e.status === "failed") {
        ipc.listJobs(0).then((rs) => {
          const r = rs.find((x) => x.id === e.job_id);
          if (r) dispatch({ type: "record", r });
        });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent(async (ev) => {
      const p = ev.payload;
      if (p.type === "enter" || p.type === "over") setHovering(true);
      else if (p.type === "leave") setHovering(false);
      else if (p.type === "drop") {
        setHovering(false);
        if (p.paths.length === 0) return;
        try {
          setError(null);
          setDropped({ paths: p.paths, pad: await ipc.inspect(p.paths) });
        } catch (e) {
          setError(String(e));
        }
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  async function start(presetId: string) {
    if (!dropped) return;
    try {
      await ipc.startJobs(presetId, dropped.paths);
      setDropped(null);
    } catch (e) {
      setError(String(e));
    }
  }

  const { active, history } = split(jobs);

  return (
    <section className={hovering ? "activity drop-hover" : "activity"} aria-label="Activity">
      <div className="dropzone" aria-live="polite">
        {hovering ? "Release to see conversions" : "Drop files here to convert them"}
      </div>
      {error && <p className="error" role="alert">{error}</p>}
      {dropped && <PresetPicker dropped={dropped} onPick={start} onClose={() => setDropped(null)} />}

      {active.length > 0 && (
        <>
          <h2>Running</h2>
          <ul className="jobs">
            {active.map((j) => (
              <JobRow key={j.id} job={j} dispatch={dispatch} />
            ))}
          </ul>
        </>
      )}

      <h2>History</h2>
      {history.length === 0 ? (
        <p className="muted">Nothing converted yet.</p>
      ) : (
        <ul className="jobs">
          {history.map((j) => (
            <JobRow key={j.id} job={j} dispatch={dispatch} />
          ))}
        </ul>
      )}
    </section>
  );
}

function PresetPicker({ dropped, onPick, onClose }: { dropped: Dropped; onPick: (id: string) => void; onClose: () => void }) {
  const { pad } = dropped;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const one = pad.files.length === 1 ? pad.files[0] : null;
  const meta = one
    ? [one.format.toUpperCase(), formatBytes(one.size), one.pages ? `${one.pages} pages` : null].filter(Boolean).join(" · ")
    : `${formatBytes(pad.files.reduce((a, f) => a + f.size, 0))} in total`;
  return (
    <div className="picker" role="dialog" aria-label="Choose a conversion">
      <header>
        <div>
          <strong>{one ? one.name : `${pad.files.length} files`}</strong>
          <div className="muted">{meta}</div>
        </div>
        <button className="ghost" onClick={onClose} aria-label="Close">
          ✕
        </button>
      </header>
      {pad.presets.length === 0 ? (
        <p className="muted">
          {pad.open_in_recast
            ? "These files are different kinds. Drop each kind separately."
            : "Recast has no conversions for this file."}
        </p>
      ) : (
        <ul className="presets">
          {pad.presets.map((p) => (
            <li key={p.id}>
              <button onClick={() => onPick(p.id)} title={p.description}>
                <span className="label">{p.label}</span>
                <span className="desc">{p.description}</span>
                {p.engine_status.state !== "installed" && <span className="badge">Downloads {p.engine_name}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function statusLine(j: JobView): string {
  switch (j.status) {
    case "queued":
      return "Waiting to start";
    case "waiting_engine":
      return `Getting the converter${j.pct != null ? ` · ${Math.round(j.pct * 100)}%` : ""}`;
    case "running":
      return j.pct != null ? [`${Math.round(j.pct * 100)}%`, formatEta(j.etaS)].filter(Boolean).join(" · ") : "Working…";
    case "done":
      return j.outPath ? `Saved as ${j.outPath.split(/[\\/]/).pop()}` : "Done";
    case "skipped":
    case "failed":
      return j.error?.message ?? "Failed";
    case "cancelled":
      return "Cancelled";
    case "interrupted":
      return "Interrupted when Recast closed";
  }
}

function JobRow({ job: j, dispatch }: { job: JobView; dispatch: Dispatch<Action> }) {
  const [open, setOpen] = useState(false);
  const running = j.status === "queued" || j.status === "waiting_engine" || j.status === "running";
  const removeRow = () => ipc.removeJob(j.id).then(() => dispatch({ type: "remove", id: j.id }));
  return (
    <li className={`job ${j.status}`}>
      <div className="job-main">
        <div className="job-title">
          <strong>{j.file}</strong> <span className="muted">→ {j.presetLabel}</span>
        </div>
        <div className="job-status">{statusLine(j)}</div>
        {running && (
          <div className={j.pct == null ? "bar indeterminate" : "bar"} role="progressbar" aria-valuenow={j.pct == null ? undefined : Math.round(j.pct * 100)}>
            <div style={{ width: `${Math.round((j.pct ?? 0) * 100)}%` }} />
          </div>
        )}
        {open && j.status === "failed" && (
          <pre className="details">{[j.error?.code, j.stderrTail].filter(Boolean).join("\n\n")}</pre>
        )}
      </div>
      <div className="job-actions">
        {running && <button onClick={() => ipc.cancelJob(j.id)}>Cancel</button>}
        {j.status === "done" && j.outPath && (
          <>
            <button onClick={() => ipc.reveal(j.outPath!)}>Show in folder</button>
            <button onClick={() => ipc.open(j.outPath!)}>Open</button>
          </>
        )}
        {j.status === "failed" && (j.error?.code === "DEST_UNWRITABLE" || j.error?.code === "DEST_LOCKED") && (
          <button onClick={() => ipc.retryJob(j.id, true)}>Save to fallback folder</button>
        )}
        {(j.status === "failed" || j.status === "cancelled" || j.status === "interrupted") && (
          <button onClick={() => ipc.retryJob(j.id)}>Retry</button>
        )}
        {j.status === "failed" && <button onClick={() => setOpen(!open)}>{open ? "Hide details" : "Details"}</button>}
        {!running && (
          <button className="ghost" onClick={removeRow} aria-label="Remove from list">
            Remove
          </button>
        )}
      </div>
    </li>
  );
}
