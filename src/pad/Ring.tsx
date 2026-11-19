/** The ring: one canvas for the glass, a small DOM overlay for the disc text and pill. */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { formatBytes } from "../shared/format";
import type { PadModel } from "../shared/types";
import { buildFrame, restSprings, springsSettled, stepRetract, stepSprings, type Spring } from "./frame";
import { OPEN_MS, PAGE_FADE_MS } from "./anim";
import { SIZE } from "./geometry";
import { clampPillX, pillAnchor, pillTop } from "./pill";
import { iconPath, kindIcon } from "./icons";
import { hitTest } from "./hitTest";
import { discText } from "./disc";
import { label } from "./progress";
import { currentPage, target, type PadKey } from "./states";
import type { Pad } from "./usePad";
import { allSlotLuminance, discLuminance } from "./luminance";
import type { GlassSource } from "./glass/backdrop";
import { draw, perfProbe } from "./glass/renderer";

export interface RingProps {
  pad: Pad;
  model: PadModel;
  thumbUrl: string | null;
  glass: GlassSource | null;
  /** Surface for the solid look when there's no glass. */
  solid: "dark" | "light";
  /** Pointer from a drag over the window (logical px), when the mouse isn't reporting. */
  external: { x: number; y: number } | null;
  /** Listen for keys on the window (the pad window, or the inline overlay while open). */
  keys: boolean;
  /** Drag-mode pad still waiting for files: "Drop files here", no tiles. */
  waiting?: boolean;
  /** Called once, after the first frame has been drawn (the pad window is shown then). */
  onFirstFrame?: () => void;
  className?: string;
}

const KEYS: Record<string, PadKey> = { ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight", Enter: "Enter", Escape: "Escape", Tab: "Tab" };
const BUSY = ["starting", "running", "done", "failed"];

export function Ring({ pad, model, thumbUrl, glass, solid, external, keys, waiting = false, onFirstFrame, className }: RingProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const pillEl = useRef<HTMLDivElement>(null);
  const [pillW, setPillW] = useState(0);
  const [pillH, setPillH] = useState(0);
  const { state, dispatch, batch, running, openedAt } = pad;
  const tgt = target(state);
  const page = currentPage(state);
  /** Tiles on show: only the slots the page has (spec §4.2). */
  const n = page.length;

  // Luminance once per backdrop and slot count (n changes only with the page).
  const lum = useMemo(
    () => (glass ? { slots: allSlotLuminance(glass.pixels, glass.scale, n), disc: discLuminance(glass.pixels, glass.scale) } : null),
    [glass, n],
  );

  // Pointer → hit test (mouse here, or an external drag position).
  const pos = external ?? pointer;
  const tgtRef = useRef(tgt);
  tgtRef.current = tgt;
  useEffect(() => {
    // dwell only for drag-sourced positions (spec §4.2): `external != null`.
    dispatch({
      type: "pointer",
      hit: pos ? hitTest(pos.x, pos.y, tgtRef.current, n) : { kind: "none" },
      at: performance.now(),
      dwell: external != null,
    });
    // `external` only changes together with `pos`; a new page (n) re-tests the same point.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pos?.x, pos?.y, n, dispatch]);

  useEffect(() => {
    if (!keys) return;
    const onKey = (e: KeyboardEvent) => {
      const k = e.key === "Tab" && e.shiftKey ? "ShiftTab" : KEYS[e.key];
      if (!k) return;
      e.preventDefault();
      dispatch({ type: "key", key: k, at: performance.now() });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keys, dispatch]);

  // The render loop (A6): runs only while something moves; the effect below restarts it
  // through `kick`; never draws while the pad is closed.
  const latest = useRef({ state, glass, solid, pos, batch, thumbUrl, lum, model, waiting, onFirstFrame });
  latest.current = { state, glass, solid, pos, batch, thumbUrl, lum, model, waiting, onFirstFrame };
  const dirty = useRef(true);
  const kick = useRef<(() => void) | null>(null);
  useEffect(() => {
    dirty.current = true;
    kick.current?.();
  }, [state, glass, solid, pos, batch, thumbUrl, lum, waiting]);
  useEffect(() => {
    const c = canvas.current!;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(SIZE * dpr);
    c.height = Math.round(SIZE * dpr);
    const ctx = c.getContext("2d")!;
    let springs: Spring[] = restSprings();
    let retract = 0;
    let last = performance.now();
    let raf = 0;
    let drawn = false;
    const probe = perfProbe(new URLSearchParams(location.search).has("perf"));
    const loop = (now: number) => {
      raf = 0;
      const dt = Math.min(64, now - last);
      last = now;
      const L = latest.current;
      const s = L.state;
      if (s.phase.name === "closed") return;
      const t = target(s);
      const busy = BUSY.includes(s.phase.name);
      springs = stepSprings(springs, busy ? null : t, dt, s.reducedMotion);
      const nextRetract = stepRetract(retract, busy, dt, s.reducedMotion);
      const moving =
        !springsSettled(springs, busy ? null : t) ||
        Math.abs(nextRetract - retract) > 0.001 ||
        now - openedAt < OPEN_MS ||
        now - s.pageAt < PAGE_FADE_MS ||
        (busy && L.batch.pct == null && !s.reducedMotion);
      retract = nextRetract;
      if (moving || dirty.current) {
        dirty.current = false;
        const frame = buildFrame({
          now, openedAt, page: currentPage(s), pageAt: s.pageAt, target: busy ? null : t, focus: s.focus,
          showFocus: s.keyboard, springs, lum: L.lum?.slots ?? null, discLum: L.lum?.disc ?? null,
          solid: L.glass ? null : L.solid, reducedMotion: s.reducedMotion, pointer: L.pos, retract,
          progress: busy ? { pct: L.batch.pct, spin: s.reducedMotion ? 0 : (now / 1000) * Math.PI * 1.6 } : null,
        });
        probe(() => draw(ctx, frame, L.glass, dpr));
        if (!drawn) {
          drawn = true;
          L.onFirstFrame?.();
        }
      }
      if (moving) raf = requestAnimationFrame(loop);
    };
    kick.current = () => {
      if (raf) return;
      last = performance.now();
      if (drawn) raf = requestAnimationFrame(loop);
      else loop(last);
    };
    dirty.current = true;
    // The first frame is drawn synchronously: a click-mode pad window is still hidden (A4), and
    // WebView2 doesn't run animation frames for a hidden window, so waiting for one would
    // deadlock `onFirstFrame` (which is what shows the window).
    loop(performance.now());
    return () => {
      kick.current = null;
      cancelAnimationFrame(raf);
    };
  }, [openedAt]);

  // A8: the ring may be scaled down (inline overlay), so divide the box size back out.
  const local = (e: MouseEvent) => {
    const r = box.current!.getBoundingClientRect();
    const k = r.width / SIZE || 1;
    return { x: (e.clientX - r.left) / k, y: (e.clientY - r.top) / k };
  };

  const text = discText(model, waiting);
  // The kind glyph shows only while choosing (opening/idle/targeting), without a thumbnail.
  const kind = !thumbUrl && !waiting && ["opening", "idle", "targeting"].includes(state.phase.name) ? (model.files[0]?.kind ?? null) : null;
  const busyNow = ["starting", "running"].includes(state.phase.name);
  const light = lum ? lum.disc > 0.6 : solid === "light";
  const tslot = tgt != null ? page[tgt] : null;
  const pill =
    tslot?.kind === "preset"
      ? [
          tslot.preset.description,
          tslot.preset.engine_status.state !== "installed" ? `Downloads ${tslot.preset.engine_name} · ${formatBytes(tslot.preset.engine_size)}` : null,
          tslot.preset.warning,
        ].filter(Boolean) as string[]
      : tslot?.kind === "more" ? ["More conversions"] : tslot?.kind === "back" ? ["Back"] : null;
  const anchor = pillAnchor(tgt ?? 0, n);
  const pillKey = pill ? pill.join("|") : "";
  useLayoutEffect(() => {
    if (pillEl.current) {
      setPillW(pillEl.current.offsetWidth);
      setPillH(pillEl.current.offsetHeight);
    }
  }, [pillKey, tgt, busyNow]);
  const pillX = clampPillX(anchor.x, pillW);
  const pillY = pillTop(anchor, pillH);
  const status =
    state.phase.name === "done" ? "Done"
      : state.phase.name === "failed" ? state.phase.message
        : busyNow ? label(batch, running) : null;

  return (
    <div
      ref={box}
      className={`ring ${light ? "on-light" : ""} ${className ?? ""}`}
      onMouseMove={(e) => setPointer(local(e))}
      onMouseLeave={() => setPointer(null)}
      onClick={(e) => {
        const p = local(e);
        dispatch({ type: "drop", hit: hitTest(p.x, p.y, tgt, n), at: performance.now() });
      }}
    >
      <canvas ref={canvas} className="ring-canvas" aria-hidden="true" />
      <div className={n === 0 ? "disc wide" : "disc"} aria-live="polite">
        {state.phase.name === "done" ? (
          <div className="disc-glyph ok" aria-hidden="true">✓</div>
        ) : state.phase.name === "failed" ? (
          <div className="disc-glyph bad" aria-hidden="true">⚠</div>
        ) : thumbUrl ? (
          <div className="thumb">
            <img src={thumbUrl} alt="" />
          </div>
        ) : kind ? (
          // The file kind's glyph, bare, in the same stack as the name (so a note can't run into it).
          <svg className="kind-glyph" viewBox="0 0 24 24" aria-hidden="true">
            <path d={iconPath(kindIcon(kind))} />
          </svg>
        ) : (
          <div className="thumb-space" />
        )}
        {status ? (
          <div className="disc-status">{status}</div>
        ) : (
          <>
            <div className="disc-title">{text.title}</div>
            {text.skipped && <div className="disc-note">{text.skipped}</div>}
            {text.note && <div className="disc-note">{text.note}</div>}
          </>
        )}
      </div>
      {pill && !busyNow && (
        <div ref={pillEl} className="pill" style={{ left: pillX, top: pillY, transform: "translate(-50%, 0)" }} role="tooltip">
          {pill.map((l) => (
            <div key={l}>{l}</div>
          ))}
        </div>
      )}
      <ul className="sr-only" role="listbox" aria-label="Conversions">
        {page.map((s, i) => (
          <li key={i} role="option" aria-selected={state.focus === i}>
            {s.kind === "preset" ? s.preset.description : s.kind === "more" ? "More conversions" : "Back"}
          </li>
        ))}
      </ul>
    </div>
  );
}
