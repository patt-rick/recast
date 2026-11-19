/** The pad window (spec §4.2–4.4, §6.8): backdrop, thumbnail, drag targeting. */
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc, onPadCursor, onPadDismiss, onPadOpen } from "../shared/ipc";
import type { PadMode, PadModel, PadOpen } from "../shared/types";
import { useThumb } from "../shared/useThumb";
import { armedAfter, outsideDistance } from "./cursor";
import { prepare, release, type GlassSource } from "./glass/backdrop";
import { planDrop, type PendingDrop } from "./dropPlan";
import { enterAction } from "./enter";
import { hitTest } from "./hitTest";
import { withOpenInRecast } from "./openInRecast";
import { Ring } from "./Ring";
import { target } from "./states";
import { usePad } from "./usePad";

interface Session {
  key: number;
  generation: number;
  paths: string[];
  mode: PadMode;
  model: PadModel;
  glass: GlassSource | null;
  /** Drag-mode pad opened with no files yet: "Drop files here", every slot empty. */
  waiting: boolean;
  /** A shake probe: invisible until a file drag enters it (Rust confirms it, B1). */
  probe: boolean;
}

const reducedMotionQuery = () => window.matchMedia("(prefers-reduced-motion: reduce)");
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");
const EMPTY: PadModel = { files: [], presets: [], open_in_recast: false };

export function PadApp() {
  const [session, setSession] = useState<Session | null>(null);
  const [external, setExternal] = useState<{ x: number; y: number } | null>(null);
  const [reducedMotion, setReducedMotion] = useState(reducedMotionQuery().matches);
  const seq = useRef(0);
  const sessionRef = useRef<Session | null>(null);
  sessionRef.current = session;
  const dragging = useRef(false);
  /** reduced_transparency of the last pad://open (A2). */
  const reducedTransparency = useRef(false);
  /** Generation of the last pad://open handled (the same open may be delivered twice under StrictMode). */
  const openGen = useRef(0);
  /** Last generation we closed: a slow `begin` must not resurrect a pad that was dismissed meanwhile. */
  const closedGen = useRef(-1);
  /** Leave-to-dismiss arms only after the cursor was inside once (or a drag entered). */
  const armed = useRef(false);
  /** Number of `begin`s awaiting inspect/capture (Rust shows a drag-mode pad before the session exists). */
  const beginning = useRef(0);
  /** A drop that arrived while a begin was in flight with no session; replayed once the session is set. */
  const pendingDrop = useRef<PendingDrop | null>(null);
  /** A file drag that entered with no session yet while a begin was in flight; replayed once it exists. */
  const pendingEnter = useRef<string[] | null>(null);
  const enterRef = useRef<(s: Session, paths: string[]) => void>(() => {});
  const dropRef = useRef<(s: Session, x: number, y: number, paths: string[]) => void>(() => {});

  useEffect(() => {
    const q = reducedMotionQuery();
    const on = () => setReducedMotion(q.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, []);

  // A7: the GlassSource is released only after the Ring that drew it has unmounted. Keyed on the
  // bitmaps, not the session, so switching to click mode (same glass) doesn't release it.
  const glass = session?.glass ?? null;
  useEffect(() => () => release(glass), [glass]);

  const finish = useCallback(() => {
    const s = sessionRef.current;
    dragging.current = false;
    setExternal(null);
    setSession(null);
    if (s) {
      closedGen.current = s.generation;
      ipc.padTrackCursor(false, s.generation).catch(() => {});
      ipc.padClose(s.generation).catch(() => {});
    }
  }, []);

  const pad = usePad({ reducedMotion, onClosed: finish });
  const padRef = useRef(pad);
  padRef.current = pad;

  const begin = useCallback(async (req: PadOpen) => {
    if (req.generation === closedGen.current) return;
    const key = ++seq.current;
    const waiting = req.mode === "drag" && req.paths.length === 0;
    const probe = req.probe && req.paths.length === 0;
    beginning.current++;
    let model: PadModel, glass: GlassSource | null;
    try {
      [model, glass] = await Promise.all([
      waiting
        ? Promise.resolve(EMPTY)
        : ipc.inspect(req.paths).catch((e) => {
            console.warn("inspect failed", e);
            return EMPTY;
          }),
      req.reduced_transparency || probe
        ? Promise.resolve(null)
        : ipc.captureBackdrop().then(prepare).catch((e) => {
            console.warn("backdrop unavailable, using the solid look", e);
            return null;
          }),
      ]);
    } finally {
      beginning.current--;
    }
    model = withOpenInRecast(model);
    if (key !== seq.current || req.generation === closedGen.current) {
      release(glass);
      if (beginning.current === 0) {
        pendingDrop.current = null;
        pendingEnter.current = null;
      }
      return;
    }
    const next: Session = { key, generation: req.generation, paths: req.paths, mode: req.mode, model, glass, waiting, probe };
    // Set the ref first: a drag entering before React renders must already see this session.
    sessionRef.current = next;
    setSession(next);
    padRef.current.open(model, req.paths);
    const enter = pendingEnter.current;
    pendingEnter.current = null;
    if (enter) enterRef.current(next, enter);
    // A drop that landed before the session existed: run it now (the reducer handles it after `open`).
    const early = pendingDrop.current;
    pendingDrop.current = null;
    if (early) dropRef.current(next, early.x, early.y, early.paths);
  }, []);

  // Listen first, then tell Rust we're ready (it may be holding an open for us).
  useEffect(() => {
    let cancelled = false;
    const offs = [
      onPadOpen((req) => {
        if (req.generation === openGen.current) return;
        openGen.current = req.generation;
        reducedTransparency.current = req.reduced_transparency;
        armed.current = req.armed;
        pendingDrop.current = null;
        pendingEnter.current = null;
        void begin(req);
      }),
      onPadCursor((p) => {
        if (!armed.current) {
          armed.current = armedAfter(false, p);
          if (!armed.current) return;
        }
        padRef.current.dispatch({ type: "outside", distance: outsideDistance(p), at: performance.now() });
      }),
      onPadDismiss(() => padRef.current.dispatch({ type: "dismiss" })),
    ];
    // Only the effect that is still mounted tells Rust it's ready (idempotent anyway).
    Promise.all(offs).then(async () => {
      if (!cancelled) await ipc.padReady();
    }).catch((e) => console.warn("pad listeners failed", e));
    return () => {
      cancelled = true;
      offs.forEach((o) => o.then((f) => f()));
    };
  }, [begin]);

  dropRef.current = (s, x, y, paths) => {
    ipc.padTrackCursor(false, s.generation).catch(() => {});
    const hit = hitTest(x, y, s === sessionRef.current ? target(padRef.current.state) : null);
    const plan = planDrop(s, paths, hit);
    if (plan === "run-and-keep" || plan === "run-and-close") {
      padRef.current.dispatch({ type: "drop", hit, at: performance.now() });
    }
    if (plan === "run-and-keep") {
      // Keep the session, but it's a click-mode pad now (Esc and click-outside close it); focus it (A2).
      setSession((cur) => (cur && cur.generation === s.generation ? { ...cur, mode: "click" } : cur));
      ipc.padShow(s.generation).catch(() => {});
    } else if (plan === "reopen") {
      void begin({ paths, mode: "click", reduced_transparency: !s.glass, generation: s.generation, probe: false, armed: true });
    }
  };

  enterRef.current = (s, paths) => {
    const action = enterAction(s, openGen.current, paths, false);
    if (action === "ignore" || action === "park") return;
    ipc.padTrackCursor(true, s.generation).catch(() => {});
    if (action === "rebegin") {
      void begin({ paths, mode: "drag", reduced_transparency: reducedTransparency.current, generation: s.generation, probe: false, armed: true });
    }
  };

  // Drags over the pad: Over positions drive targeting; Drop runs the targeted segment.
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const scale = window.devicePixelRatio || 1;
      const s = sessionRef.current;
      if (p.type === "enter") {
        dragging.current = true;
        armed.current = true;
        const action = enterAction(s, openGen.current, p.paths, beginning.current > 0);
        if (action === "park") pendingEnter.current = p.paths;
        else if (s) enterRef.current(s, p.paths);
      } else if (p.type === "over") {
        setExternal({ x: p.position.x / scale, y: p.position.y / scale });
      } else if (p.type === "leave") {
        dragging.current = false;
        setExternal(null);
      } else if (p.type === "drop") {
        dragging.current = false;
        setExternal(null);
        // No session yet, or the session showing is older than an open still loading:
        // park the drop for that open (acting on the old one would revive a stale generation).
        if (!s || s.generation !== openGen.current) {
          if (beginning.current > 0) pendingDrop.current = { x: p.position.x / scale, y: p.position.y / scale, paths: p.paths };
          return;
        }
        dropRef.current(s, p.position.x / scale, p.position.y / scale, p.paths);
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [begin]);

  // Click mode: clicking anywhere else (the pad loses focus) dismisses it (§4.3).
  useEffect(() => {
    const off = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused && sessionRef.current?.mode === "click" && !dragging.current) {
        padRef.current.dispatch({ type: "dismiss" });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  const thumb = useThumb(session?.model.files[0]?.path ?? null);
  if (!session || session.probe) return null;
  return (
    <Ring
      key={session.key}
      pad={pad}
      model={session.model}
      thumbUrl={thumb}
      glass={session.glass}
      solid={darkQuery().matches ? "dark" : "light"}
      external={external}
      waiting={session.waiting}
      keys
      onFirstFrame={() => {
        // A4: a click-mode pad is shown (and focused) only once it has drawn; drag mode is already shown.
        const s = sessionRef.current;
        if (s && s.mode === "click") ipc.padShow(s.generation).catch(() => {});
      }}
    />
  );
}
