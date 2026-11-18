/** The ring inside the companion (spec §4.5 item 1): solid look, click or keyboard. */
import { useEffect, useState } from "react";
import type { PadModel } from "../shared/types";
import { useThumb } from "../shared/useThumb";
import { SIZE } from "../pad/geometry";
import { Ring } from "../pad/Ring";
import { usePad } from "../pad/usePad";

const mq = (q: string) => window.matchMedia(q);

/** A8: scale the ring down when the window is shorter than it. Hit-testing divides this back out. */
function useFitScale(): number {
  const calc = () => Math.min(1, (window.innerHeight - 20) / SIZE, (window.innerWidth - 20) / SIZE);
  const [k, setK] = useState(calc);
  useEffect(() => {
    const on = () => setK(calc());
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return Math.max(0.3, k);
}

export function InlineRing({ paths, model, onClose }: { paths: string[]; model: PadModel; onClose: () => void }) {
  const [reducedMotion] = useState(() => mq("(prefers-reduced-motion: reduce)").matches);
  const pad = usePad({ reducedMotion, onClosed: onClose });
  const { open } = pad;
  // Runs twice under StrictMode in dev, which is harmless.
  useEffect(() => {
    open(model, paths);
  }, [model, paths, open]);
  const thumb = useThumb(model.files[0]?.path ?? null);
  const scale = useFitScale();
  return (
    <div
      className="ring-overlay"
      role="dialog"
      aria-label="Choose a conversion"
      onClick={(e) => {
        if (e.target === e.currentTarget) pad.dispatch({ type: "dismiss" });
      }}
    >
      <div className="ring-fit" style={{ transform: `scale(${scale})` }}>
        <Ring pad={pad} model={model} thumbUrl={thumb} glass={null}
          solid={mq("(prefers-color-scheme: dark)").matches ? "dark" : "light"} external={null} keys />
      </div>
    </div>
  );
}
