import { useEffect, useState, type ReactNode } from "react";
import { ipc } from "../shared/ipc";
import type { DeepPartial, Settings } from "../shared/types";

export function SettingsView() {
  const [s, setS] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc.getSettings().then(setS).catch((e) => setError(String(e)));
  }, []);

  const save = (patch: DeepPartial<Settings>) =>
    ipc.setSettings(patch).then(setS).catch((e) => setError(String(e)));

  if (!s) return <p className="muted">{error ?? "Loading…"}</p>;

  return (
    <section aria-label="Settings">
      {error && <p className="error" role="alert">{error}</p>}

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
    </section>
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
