import { useEffect, useState, type ReactNode } from "react";
import type { DeepPartial, Settings } from "../shared/types";

type Save = (p: DeepPartial<Settings>) => Promise<unknown>;

export function SettingsView({ s, error, save }: { s: Settings | null; error: string | null; save: Save }) {
  if (!s) return <p className="muted">{error ?? "Loading…"}</p>;

  return (
    <section aria-label="Settings">
      {error && <p className="error" role="alert">{error}</p>}

      <h2>Opening the pad</h2>
      <TriggerFields s={s} save={save} />

      <h2>Output</h2>
      <Field label="Save converted files">
        <select
          value={s.output.location}
          onChange={(e) => save({ output: { location: e.target.value as Settings["output"]["location"] } })}
        >
          <option value="next_to_original">Next to the original</option>
          <option value="folder">In a fixed folder</option>
        </select>
      </Field>
      {s.output.location === "folder" && (
        <Field label="Folder">
          <TextInput value={s.output.folder} placeholder="C:\Users\you\Converted" onCommit={(v) => save({ output: { folder: v } })} />
        </Field>
      )}
      <Field label="Fallback folder">
        <TextInput value={s.output.fallback_folder} placeholder="Documents\Recast" onCommit={(v) => save({ output: { fallback_folder: v } })} />
      </Field>

      <h2>Quality</h2>
      <Field label="MP3 bitrate">
        <Kbps value={s.quality.mp3_kbps} onChange={(v) => save({ quality: { mp3_kbps: v } })} />
      </Field>
      <Field label="AAC / M4A bitrate">
        <Kbps value={s.quality.aac_kbps} onChange={(v) => save({ quality: { aac_kbps: v } })} />
      </Field>
      <Field label="JPG quality (1–100)">
        <NumberInput value={s.quality.jpg_q} min={1} max={100} onCommit={(v) => save({ quality: { jpg_q: v } })} />
      </Field>
      <Field label="WebP quality (1–100)">
        <NumberInput value={s.quality.webp_q} min={1} max={100} onCommit={(v) => save({ quality: { webp_q: v } })} />
      </Field>
      <Field label="AVIF quality (1–100)">
        <NumberInput value={s.quality.avif_q} min={1} max={100} onCommit={(v) => save({ quality: { avif_q: v } })} />
      </Field>
      <Field label="GIF frames per second">
        <NumberInput value={s.gif.fps} min={1} max={50} onCommit={(v) => save({ gif: { fps: v } })} />
      </Field>
      <Field label="GIF width (px)">
        <NumberInput value={s.gif.width} min={16} max={4096} onCommit={(v) => save({ gif: { width: v } })} />
      </Field>
      <Field label="Shrink video to (MB)">
        <NumberInput value={s.shrink.target_mb} min={1} max={100000} onCommit={(v) => save({ shrink: { target_mb: v } })} />
      </Field>
      <Field label="Shrink images to (longest edge, px)">
        <NumberInput value={s.shrink.image_px} min={16} max={16384} onCommit={(v) => save({ shrink: { image_px: v } })} />
      </Field>

      <h2>Jobs</h2>
      <Field label="Run at the same time">
        <select value={s.jobs.concurrency} onChange={(e) => save({ jobs: { concurrency: Number(e.target.value) } })}>
          {[1, 2, 3, 4].map((n) => (
            <option key={n} value={n}>{n}</option>
          ))}
        </select>
      </Field>
      <Field label="Stop a stuck job after (seconds)">
        <NumberInput value={s.jobs.stall_timeout_s} min={10} max={3600} onCommit={(v) => save({ jobs: { stall_timeout_s: v } })} />
      </Field>

      <h2>Windows</h2>
      <Toggle label="“Open with Recast” in Explorer" hint="Right-click files, then Show more options."
        checked={s.shell.explorer_entry} onChange={(v) => save({ shell: { explorer_entry: v } })} />
      <Toggle label="Start with Windows" hint="Recast waits in the tray, ready for a drop."
        checked={s.app.start_with_windows} onChange={(v) => save({ app: { start_with_windows: v } })} />
      <p className="muted">Drags from apps running as administrator can't reach Recast; Windows blocks them.</p>
    </section>
  );
}

export function TriggerFields({ s, save }: { s: Settings; save: Save }) {
  const t = s.triggers;
  return (
    <>
      <Toggle label="Edge handle" hint="A thin tab on the screen edge. Drag a file onto it to open the pad."
        checked={t.edge_handle} onChange={(v) => save({ triggers: { edge_handle: v } })} />
      {t.edge_handle && (
        <>
          <Field label="Edge">
            <select value={t.edge} onChange={(e) => save({ triggers: { edge: e.target.value as Settings["triggers"]["edge"] } })}>
              <option value="left">Left</option>
              <option value="right">Right</option>
              <option value="top">Top</option>
              <option value="bottom">Bottom</option>
            </select>
          </Field>
          <Field label={`Position along the edge (${Math.round(t.edge_pos * 100)}%)`}>
            <input type="range" min={0} max={100} step={5} value={Math.round(t.edge_pos * 100)}
              onChange={(e) => save({ triggers: { edge_pos: Number(e.target.value) / 100 } })} />
          </Field>
        </>
      )}
      <Toggle label="Desk pad" hint="A round pad on your desktop that you can drop files on. Drag it to move it."
        checked={t.desk_pad} onChange={(v) => save({ triggers: { desk_pad: v } })} />
      <Toggle label="Shake to open" hint="Shake a file quickly left and right while you drag it. Off while a fullscreen app is in front."
        checked={t.shake} onChange={(v) => save({ triggers: { shake: v } })} />
    </>
  );
}

function Toggle({ label, hint, checked, onChange }: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="field toggle">
      <span>
        {label}
        {hint && <small className="muted">{hint}</small>}
      </span>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function Kbps({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  return (
    <select value={value} onChange={(e) => onChange(Number(e.target.value))}>
      {[96, 128, 160, 192, 256, 320].map((k) => (
        <option key={k} value={k}>{k} kbps</option>
      ))}
    </select>
  );
}

/** Commits on blur or Enter, so typing doesn't save every keystroke. */
function NumberInput({ value, min, max, onCommit }: { value: number; min: number; max: number; onCommit: (v: number) => void }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(draft));
    if (Number.isFinite(n) && n >= min && n <= max && n !== value) onCommit(n);
    else setDraft(String(value));
  };
  return (
    <input
      type="number"
      min={min}
      max={max}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

function TextInput({ value, placeholder, onCommit }: { value: string; placeholder: string; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => draft !== value && onCommit(draft.trim());
  return (
    <input
      type="text"
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}
