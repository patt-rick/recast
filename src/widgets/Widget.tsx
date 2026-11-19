/** The edge handle and the desk pad (spec §4.1). Rust handles their drag-drop events. */
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Highlight = { on: boolean; edge: string };

export function Widget() {
  const [label] = useState(() => getCurrentWindow().label);
  const [hover, setHover] = useState(false);
  const [highlight, setHighlight] = useState<Highlight>({ on: false, edge: "left" });
  useEffect(() => {
    const offs = [
      listen<boolean>("widget://hover", (e) => setHover(e.payload)),
      listen<Highlight>("widget://highlight", (e) => setHighlight(e.payload)),
    ];
    return () => offs.forEach((o) => o.then((f) => f()));
  }, []);
  const cls = (base: string) => [base, hover && "hover", highlight.on && "highlight"].filter(Boolean).join(" ");
  if (label === "deskpad") {
    return (
      <div className={cls("desk")} data-tauri-drag-region title="Drop files here to convert them">
        <img src="/icon.svg" alt="" draggable={false} />
      </div>
    );
  }
  // While highlighted the window is bigger: the tab stays flush with the edge and the
  // label sits beside it, toward the screen centre. The rest is transparent.
  return (
    <div className={`handle-box edge-${highlight.edge}`}>
      <div className={cls("handle")} title="Drag a file here" />
      {highlight.on && <div className="handle-label">Drag a file here</div>}
    </div>
  );
}
