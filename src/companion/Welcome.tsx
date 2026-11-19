/** First run (spec §4.6): the edge handle, the tray icon, the trigger toggles. */
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc } from "../shared/ipc";
import type { DeepPartial, Settings } from "../shared/types";
import type { SaveResult } from "./saveQueue";
import { TriggerFields } from "./Settings";
import { shouldHighlight } from "./welcomeState";

/** Is the companion on screen and in front? The handle only pulses for a window someone can see. */
function useWindowActive(): { visible: boolean; focused: boolean } {
  const [st, setSt] = useState({ visible: false, focused: false });
  useEffect(() => {
    const win = getCurrentWindow();
    let live = true;
    const check = () =>
      Promise.all([win.isVisible(), win.isFocused()])
        .then(([visible, focused]) => live && setSt({ visible, focused }))
        .catch(() => {});
    void check();
    const off = win.onFocusChanged(({ payload }) => {
      if (payload) void check();
      else setSt((s) => ({ ...s, focused: false }));
    });
    return () => {
      live = false;
      off.then((f) => f()).catch(() => {});
    };
  }, []);
  return st;
}

export function Welcome({ s, save, onDone }: { s: Settings; save: (p: DeepPartial<Settings>) => Promise<SaveResult>; onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const { visible, focused } = useWindowActive();
  const highlight = shouldHighlight({ step, visible, focused });
  useEffect(() => {
    ipc.highlightHandle(highlight).catch(() => {});
  }, [highlight]);
  useEffect(() => () => void ipc.highlightHandle(false).catch(() => {}), []);
  const finish = async () => {
    if (saving) return;
    setSaving(true);
    setSaveError(null);
    const r = await save({ app: { first_run_done: true } });
    setSaving(false);
    if (r.ok) onDone();
    else setSaveError(`Couldn't save that: ${r.error}`);
  };
  const steps = [
    {
      title: "Drag a file here",
      body: s.triggers.edge_handle
        ? `The glowing tab on the ${s.triggers.edge} edge of your screen is the edge handle. Drag any file onto it and the pad opens beside it. Release on a conversion and the new file appears next to the original.`
        : "The edge handle is off. You can turn it on in the last step.",
    },
    {
      title: "Recast lives in the tray",
      body: "Recast keeps running in the notification area by the clock. Click its icon to open this window. Right-click it for Convert files… and Quit.",
    },
    { title: "Choose how the pad opens", body: null },
  ];
  const cur = steps[step];
  return (
    <div className="welcome-overlay" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <div className="welcome">
        <p className="muted">Welcome to Recast · {step + 1} of 3</p>
        <h2 id="welcome-title">{cur.title}</h2>
        {cur.body ? <p>{cur.body}</p> : <TriggerFields s={s} save={save} />}
        {saveError && <p className="error" role="alert">{saveError}</p>}
        <div className="welcome-actions">
          <button className="ghost" onClick={() => void finish()} disabled={saving}>Skip</button>
          {step < 2 ? <button onClick={() => setStep(step + 1)} autoFocus>Next</button> : <button onClick={() => void finish()} disabled={saving} autoFocus>Done</button>}
        </div>
      </div>
    </div>
  );
}
