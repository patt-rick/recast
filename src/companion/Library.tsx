import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc, onEngineEvent } from "../shared/ipc";
import { formatBytes } from "../shared/format";
import type { EngineRow, Notice } from "../shared/types";

export function Library() {
  const [rows, setRows] = useState<EngineRow[]>([]);
  const [notices, setNotices] = useState<Notice[]>([]);
  const [messages, setMessages] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  // Licence texts currently expanded, by notice name.
  const [licences, setLicences] = useState<Record<string, string>>({});

  const toggleLicence = async (index: number, name: string) => {
    if (name in licences) {
      setLicences(({ [name]: _gone, ...rest }) => rest);
      return;
    }
    try {
      const text = await ipc.noticeText(index, name);
      setLicences((l) => ({ ...l, [name]: text }));
    } catch (e) {
      setError(String(e));
    }
  };

  const say = (id: string, msg: string | null) =>
    setMessages((m) => {
      const next = { ...m };
      if (msg) next[id] = msg;
      else delete next[id];
      return next;
    });

  const refresh = useCallback(() => {
    ipc.enginesList().then(setRows).catch((e) => setError(String(e)));
    ipc.notices().then(setNotices).catch((e) => setError(String(e)));
  }, []);

  const setBusy = (id: string, on: boolean) =>
    setPending((p) => {
      const next = new Set(p);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  // Download / Repair / Update stay disabled from the click until the engine reports done or failed.
  const install = (id: string) => {
    setBusy(id, true);
    ipc.engineInstall(id).catch((e) => {
      say(id, String(e));
      setBusy(id, false);
    });
  };

  useEffect(refresh, [refresh]);

  useEffect(() => {
    const off = onEngineEvent((e) => {
      if (e.kind === "progress") {
        setRows((rs) => rs.map((r) => (r.id === e.id ? { ...r, status: { state: "installing", pct: e.pct } } : r)));
      } else {
        say(e.id, e.kind === "failed" ? e.error : null);
        setBusy(e.id, false);
        refresh();
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [refresh]);

  const verify = async (id: string) => {
    try {
      const ok = await ipc.engineVerify(id);
      say(id, ok ? "Verified: files match what was installed." : "Damaged: download it again to repair.");
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const total = rows.reduce((a, r) => a + r.disk_bytes, 0);

  return (
    <section aria-label="Library">
      {error && <p className="error" role="alert">{error}</p>}
      <h2>Converters</h2>
      <ul className="rows">
        {rows.map((r) => {
          const busy = pending.has(r.id) || r.status.state === "installing";
          return (
          <li key={r.id} className="row">
            <div>
              <strong>{r.name}</strong> <span className="muted">{r.version} · {r.licence}</span>
              <div className="muted">{describe(r)}</div>
              {messages[r.id] && <div className="muted" role="status">{messages[r.id]}</div>}
            </div>
            <div className="job-actions">
              {r.status.state === "not_installed" && <button disabled={busy} onClick={() => install(r.id)}>Download ({formatBytes(r.size)})</button>}
              {r.status.state === "damaged" && <button disabled={busy} onClick={() => install(r.id)}>Repair</button>}
              {r.status.state === "installed" && r.status.update_available && <button disabled={busy} onClick={() => install(r.id)}>Update</button>}
              {r.status.state === "installed" && <button onClick={() => verify(r.id)}>Verify</button>}
              {(r.status.state === "installed" || r.status.state === "damaged") && (
                <button className="ghost" onClick={() => ipc.engineRemove(r.id).then(refresh, (e) => say(r.id, String(e)))}>Remove</button>
              )}
            </div>
          </li>
          );
        })}
        <li className="row muted">
          <span>LibreOffice (Office documents), LaTeX (PDF from text)</span>
          <span>Coming later</span>
        </li>
      </ul>
      <p className="muted">Disk used: {formatBytes(total)}</p>

      <h2>Third-party notices</h2>
      <ul className="rows">
        {notices.map((n, i) => (
          <li key={n.name} className="row">
            <div>
              <strong>{n.name}</strong> <span className="muted">{n.licence}{n.bundled ? " · included with Recast" : " · downloaded from upstream"}</span>
              {n.name in licences && <pre className="licence-text" tabIndex={0} aria-label={`${n.name} licence`}>{licences[n.name]}</pre>}
            </div>
            <div className="job-actions">
              <button aria-expanded={n.name in licences} onClick={() => toggleLicence(i, n.name)}>Licence</button>
              <button onClick={() => openUrl(n.homepage)}>Website</button>
              {n.source_url && <button onClick={() => openUrl(n.source_url!)}>Source</button>}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

function describe(r: EngineRow): string {
  switch (r.status.state) {
    case "not_installed":
      return "Not installed. Downloads the first time you need it.";
    case "installing":
      return `Installing · ${Math.round(r.status.pct * 100)}%`;
    case "installed":
      return `Installed${r.status.update_available ? ` (${r.status.version}; ${r.version} available)` : ""} · ${formatBytes(r.disk_bytes)}`;
    case "damaged":
      return "Damaged";
  }
}
