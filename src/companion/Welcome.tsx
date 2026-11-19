/** First run (spec §4.6): the edge handle, the tray icon, the trigger toggles. */
import { useEffect, useState } from "react";
import { ipc } from "../shared/ipc";
import type { DeepPartial, Settings } from "../shared/types";
import { TriggerFields } from "./Settings";

export function Welcome({ s, save, onDone }: { s: Settings; save: (p: DeepPartial<Settings>) => Promise<unknown>; onDone: () => void }) {
  const [step, setStep] = useState(0);
  useEffect(() => {
    ipc.highlightHandle(step === 0).catch(() => {});
  }, [step]);
  useEffect(() => () => void ipc.highlightHandle(false).catch(() => {}), []);
  const finish = () => {
    void save({ app: { first_run_done: true } }).finally(onDone);
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
        <div className="welcome-actions">
          <button className="ghost" onClick={finish}>Skip</button>
          {step < 2 ? <button onClick={() => setStep(step + 1)} autoFocus>Next</button> : <button onClick={finish} autoFocus>Done</button>}
        </div>
      </div>
    </div>
  );
}
