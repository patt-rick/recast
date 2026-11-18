/** The pad window (spec §4.2–4.4, §6.8): backdrop, thumbnail, drag targeting. */
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc, onPadCursor, onPadDismiss, onPadOpen } from "../shared/ipc";
import type { PadMode, PadModel, PadOpen } from "../shared/types";
import { useThumb } from "../shared/useThumb";
import { armedAfter, outsideDistance } from "./cursor";
import { prepare, release, type GlassSource } from "./glass/backdrop";
import { hitTest } from "./hitTest";
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
}

const reducedMotionQuery = () => window.matchMedia("(prefers-reduced-motion: reduce)");
const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");
const samePaths = (a: string[], b: string[]) => a.length === b.length && a.every((p, i) => p === b[i]);
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
    const [model, glass] = await Promise.all([
      waiting
        ? Promise.resolve(EMPTY)
        : ipc.inspect(req.paths).catch((e) => {
            console.warn("inspect failed", e);
            return EMPTY;
          }),
      req.reduced_transparency
        ? Promise.resolve(null)
        : ipc.captureBackdrop().then(prepare).catch((e) => {
            console.warn("backdrop unavailable, using the solid look", e);
            return null;
          }),
    ]);
    if (key !== seq.current || req.generation === closedGen.current) {
      release(glass);
      return;
    }
    setSession({ key, generation: req.generation, paths: req.paths, mode: req.mode, model, glass, waiting });
    padRef.current.open(model, req.paths);
  }, []);

  // Listen first, then tell Rust we're ready (it may be holding an open for us).
  useEffect(() => {
    let cancelled = false;
    const offs = [
      onPadOpen((req) => {
        if (req.generation === openGen.current) return;
        openGen.current = req.generation;
        reducedTransparency.current = req.reduced_transparency;
        armed.current = false;
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

  // Drags over the pad: Over positions drive targeting; Drop runs the targeted segment.
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const scale = window.devicePixelRatio || 1;
      const s = sessionRef.current;
      if (p.type === "enter") {
        dragging.current = true;
        armed.current = true;
        if (!s) return;
        ipc.padTrackCursor(true, s.generation).catch(() => {});
        // Dragged files differ from the session's: target the files being dragged (A2).
        if (!samePaths(p.paths, s.paths) && p.paths.length > 0) {
          void begin({ paths: p.paths, mode: "drag", reduced_transparency: reducedTransparency.current, generation: s.generation });
        }
      } else if (p.type === "over") {
        setExternal({ x: p.position.x / scale, y: p.position.y / scale });
      } else if (p.type === "leave") {
        dragging.current = false;
        setExternal(null);
      } else if (p.type === "drop") {
        dragging.current = false;
        setExternal(null);
        if (!s) return;
        ipc.padTrackCursor(false, s.generation).catch(() => {});
        const x = p.position.x / scale, y = p.position.y / scale;
        if (!s.waiting && samePaths(p.paths, s.paths)) {
          padRef.current.dispatch({ type: "drop", hit: hitTest(x, y, target(padRef.current.state)), at: performance.now() });
          // Keep the session, but it's a click-mode pad now (Esc and click-outside close it); focus it (A2).
          setSession((cur) => (cur && cur.generation === s.generation ? { ...cur, mode: "click" } : cur));
          ipc.padShow(s.generation).catch(() => {});
        } else if (p.paths.length > 0) {
          void begin({ paths: p.paths, mode: "click", reduced_transparency: !s.glass, generation: s.generation });
        }
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
  if (!session) return null;
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
