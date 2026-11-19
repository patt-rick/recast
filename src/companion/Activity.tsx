import { useCallback, useEffect, useReducer, useRef, useState, type Dispatch } from "react";
import { ipc, onJobEvent } from "../shared/ipc";
import { formatEta } from "../shared/format";
import type { JobEvent, JobRecord, Kind, PadModel } from "../shared/types";
import { router } from "./dragRouter";
import { groupByKind, type Group } from "./groups";
import { InlineRing } from "./InlineRing";
import { appendHistory, applyEvent, batchSuffix, empty, nextCursor, PAGE_SIZE, remove, seed, split, withRecord, type JobsState, type JobView } from "./jobs";

type Action =
  | { type: "seed"; records: JobRecord[] }
  | { type: "more"; records: JobRecord[] }
  | { type: "event"; e: JobEvent }
  | { type: "record"; r: JobRecord }
  | { type: "remove"; id: string };

function reducer(s: JobsState, a: Action): JobsState {
  switch (a.type) {
    case "seed":
      return seed(s, a.records);
    case "more":
      return appendHistory(s, a.records).state;
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
  /** Set when this came from a kind group of a mixed drop. */
  kind?: Kind;
}

export function Activity({ focusJob, incoming }: { focusJob?: { id: string; n: number } | null; incoming?: { paths: string[]; n: number } | null }) {
  const [jobs, dispatch] = useReducer(reducer, empty);
  const [dropped, setDropped] = useState<Dropped | null>(null);
  const [groups, setGroups] = useState<Group[]>([]);
  const [hovering, setHovering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Smallest history seq loaded so far (keyset cursor), and whether the last page was full.
  const [cursor, setCursor] = useState<number | undefined>(undefined);
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const report = (e: unknown) => setError(String(e));

  useEffect(() => {
    ipc.listJobs().then((records) => {
      dispatch({ type: "seed", records });
      setCursor((c) => nextCursor(c, records));
      setHasMore(records.length >= PAGE_SIZE);
    }).catch((e) => setError(String(e)));
  }, []);

  async function loadMore() {
    if (loadingMore) return;
    setLoadingMore(true);
    try {
      const records = await ipc.listJobs(cursor);
      dispatch({ type: "more", records });
      setCursor(nextCursor(cursor, records));
      setHasMore(records.length >= PAGE_SIZE);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }

  useEffect(() => {
    const off = onJobEvent((e) => {
      dispatch({ type: "event", e });
      if (e.status === "failed") {
        ipc.listJobs().then((rs) => {
          const r = rs.find((x) => x.id === e.job_id);
          if (r) dispatch({ type: "record", r });
        }).catch((err) => setError(String(err)));
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  // One path for drops and for files the pad handed over: single-kind sets get the ring, mixed ones the groups.
  const receive = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    try {
      setError(null);
      const pad = await ipc.inspect(paths);
      if (pad.open_in_recast) {
        setDropped(null);
        setGroups(groupByKind(pad.files));
      } else {
        setGroups([]);
        setDropped({ paths, pad });
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(
    () =>
      router.subscribe("activity", (ev) => {
        if (ev.type === "enter" || ev.type === "over") setHovering(true);
        else if (ev.type === "leave") setHovering(false);
        else {
          setHovering(false);
          void receive(ev.paths);
        }
      }),
    [receive],
  );

  useEffect(() => {
    if (incoming) void receive(incoming.paths);
  }, [incoming, receive]);

  const openGroup = async (g: Group) => {
    try {
      setDropped({ paths: g.paths, pad: await ipc.inspect(g.paths), kind: g.kind });
    } catch (e) {
      setError(String(e));
    }
  };

  const { active, history } = split(jobs);

  return (
    <section className={hovering ? "activity drop-hover" : "activity"} aria-label="Activity">
      <div className="dropzone" aria-live="polite">
        {hovering ? "Release to see conversions" : "Drop files here to convert them"}
      </div>
      {error && <p className="error" role="alert">{error}</p>}
      {groups.length > 0 && (
        <div className="groups" aria-label="Dropped files by kind">
          <p>These files are different kinds. Convert each group on its own.</p>
          <ul>
            {groups.map((g) => (
              <li key={g.kind}>
                <span>
                  <strong>{g.label}</strong>{" "}
                  <span className="muted">{g.names.slice(0, 3).join(", ")}{g.names.length > 3 ? "…" : ""}</span>
                </span>
                <button disabled={g.kind === "unknown"} title={g.kind === "unknown" ? "Recast has no conversions for these" : undefined}
                  onClick={() => openGroup(g)}>
                  Choose conversion
                </button>
              </li>
            ))}
          </ul>
          <button className="ghost" onClick={() => setGroups([])}>Clear</button>
        </div>
      )}
      {dropped && (
        <InlineRing
          paths={dropped.paths}
          model={dropped.pad}
          onClose={() => setDropped(null)}
          onStarted={() => {
            const k = dropped.kind;
            if (k) setGroups((gs) => gs.filter((x) => x.kind !== k));
          }}
        />
      )}

      {active.length > 0 && (
        <>
          <h2>Running</h2>
          <ul className="jobs">
            {active.map((j) => (
              <JobRow key={j.id} job={j} dispatch={dispatch} report={report} focused={focusJob?.id === j.id ? focusJob.n : 0} />
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
            <JobRow key={j.id} job={j} dispatch={dispatch} report={report} focused={focusJob?.id === j.id ? focusJob.n : 0} />
          ))}
        </ul>
      )}
      {hasMore && (
        <button className="load-more" disabled={loadingMore} onClick={loadMore}>
          {loadingMore ? "Loading…" : "Load more"}
        </button>
      )}
    </section>
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

function JobRow({ job: j, dispatch, report, focused }: { job: JobView; dispatch: Dispatch<Action>; report: (e: unknown) => void; focused: number }) {
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    if (!focused) return;
    setOpen(true);
    rowRef.current?.scrollIntoView({ block: "center" });
  }, [focused]);
  const running = j.status === "queued" || j.status === "waiting_engine" || j.status === "running";
  const [retrying, setRetrying] = useState(false);
  const retry = async (fallback?: boolean) => {
    if (retrying) return;
    setRetrying(true);
    try {
      await ipc.retryJob(j.id, fallback);
    } catch (e) {
      report(e);
    } finally {
      setRetrying(false);
    }
  };
  const removeRow = () => ipc.removeJob(j.id).then(() => dispatch({ type: "remove", id: j.id }), report);
  return (
    <li ref={rowRef} className={`job ${j.status}`}>
      <div className="job-main">
        <div className="job-title">
          <strong>{j.file}</strong> <span className="muted">→ {j.presetLabel}</span>
        </div>
        <div className="job-status">{statusLine(j) + batchSuffix(j)}</div>
        {running && (
          <div className={j.pct == null ? "bar indeterminate" : "bar"} role="progressbar" aria-label={`${j.file} progress`} aria-valuenow={j.pct == null ? undefined : Math.round(j.pct * 100)}>
            <div style={{ width: `${Math.round((j.pct ?? 0) * 100)}%` }} />
          </div>
        )}
        {open && j.status === "failed" && (
          <pre className="details">{[j.error?.code, j.stderrTail].filter(Boolean).join("\n\n")}</pre>
        )}
      </div>
      <div className="job-actions">
        {running && <button onClick={() => ipc.cancelJob(j.id).catch(report)}>Cancel</button>}
        {j.status === "done" && j.outPath && (
          <>
            <button onClick={() => ipc.revealJob(j.id).catch(report)}>Show in folder</button>
            <button onClick={() => ipc.openJob(j.id).catch(report)}>Open</button>
          </>
        )}
        {j.status === "failed" && (j.error?.code === "DEST_UNWRITABLE" || j.error?.code === "DEST_LOCKED") && (
          <button disabled={retrying} onClick={() => retry(true)}>Save to fallback folder</button>
        )}
        {(j.status === "failed" || j.status === "cancelled" || j.status === "interrupted") && (
          <button disabled={retrying} onClick={() => retry()}>Retry</button>
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
