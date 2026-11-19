/** The pad's state machine wired to IPC: starting jobs, following them, ticking timers. */
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { ipc, onJobEvent } from "../shared/ipc";
import type { PadModel, PresetView } from "../shared/types";
import { startAction } from "./openInRecast";
import { paginate } from "./paging";
import { emptyBatch, view, withEvent, withIds, type Batch, type BatchView } from "./progress";
import { closed, reduce, type PadEvent, type PadState } from "./states";

export interface Pad {
  state: PadState;
  dispatch: (e: PadEvent) => void;
  batch: BatchView;
  /** The preset being run (for the engine download label). */
  running: PresetView | null;
  openedAt: number;
  open: (model: PadModel, paths: string[]) => void;
}

export function usePad(opts: { reducedMotion: boolean; onClosed: () => void }): Pad {
  const [state, dispatch] = useReducer(reduce, closed);
  const [batch, setBatch] = useState<Batch>(emptyBatch);
  const [openedAt, setOpenedAt] = useState(0);
  const paths = useRef<string[]>([]);
  const model = useRef<PadModel | null>(null);
  const following = useRef(false);
  const runningId = useRef<string | null>(null);
  const onClosed = useRef(opts.onClosed);
  onClosed.current = opts.onClosed;
  const wasOpen = useRef(false);

  const open = useCallback(
    (m: PadModel, p: string[]) => {
      model.current = m;
      paths.current = p;
      following.current = false;
      runningId.current = null;
      setBatch(emptyBatch());
      const at = performance.now();
      setOpenedAt(at);
      dispatch({ type: "open", pages: paginate(m.presets), at, reducedMotion: opts.reducedMotion });
    },
    [opts.reducedMotion],
  );

  // Starting: events can arrive before start_jobs returns, so follow from now on.
  const phase = state.phase;
  useEffect(() => {
    if (phase.name !== "starting") return;
    if (startAction(phase.presetId) === "openInRecast") {
      ipc.openInRecast(paths.current).catch((e) => console.warn("open in Recast failed", e));
      dispatch({ type: "dismiss" });
      return;
    }
    runningId.current = phase.presetId;
    following.current = true;
    let live = true;
    ipc.startJobs(phase.presetId, paths.current).then(
      (ids) => {
        if (!live) return;
        setBatch((b) => withIds(b, ids));
        dispatch({ type: "started" });
      },
      (e) => live && dispatch({ type: "startFailed", message: String(e), at: performance.now() }),
    );
    return () => {
      live = false;
    };
  }, [phase]);

  useEffect(() => {
    const off = onJobEvent((e) => {
      if (following.current) setBatch((b) => withEvent(b, e));
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  const v = view(batch);
  useEffect(() => {
    if (phase.name === "running" && v.settled) {
      following.current = false;
      dispatch({ type: "settled", failed: v.failed, at: performance.now() });
    }
  }, [phase.name, v.settled, v.failed]);

  // Timers (open → idle, dwell, leave, done/failed) run off a 50 ms tick while open.
  const isOpen = phase.name !== "closed";
  useEffect(() => {
    if (!isOpen) return;
    const t = window.setInterval(() => dispatch({ type: "tick", at: performance.now() }), 50);
    return () => window.clearInterval(t);
  }, [isOpen]);

  useEffect(() => {
    if (isOpen) wasOpen.current = true;
    else if (wasOpen.current) {
      wasOpen.current = false;
      following.current = false;
      onClosed.current();
    }
  }, [isOpen]);

  const running = ["starting", "running", "done", "failed"].includes(phase.name)
    ? (model.current?.presets.find((p) => p.id === runningId.current) ?? null)
    : null;

  return { state, dispatch, batch: v, running, openedAt, open };
}
