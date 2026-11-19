/** Tools (spec §4.5 item 2): ordered multi-file jobs and Split by page ranges. */
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "../shared/ipc";
import { parseRanges } from "../shared/pageRanges";
import type { Kind } from "../shared/types";
import { router } from "./dragRouter";
import { SortableList } from "./SortableList";
import { accept, rejection, type ToolItem } from "./toolList";

type ToolId = "merge" | "images" | "split";
const IMAGE_EXTS = ["jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "gif"];
const TOOLS: Record<ToolId, { title: string; kind: Kind; hint: string; exts: string[] }> = {
  merge: { title: "Merge PDFs", kind: "pdf", hint: "Put the PDFs in order, then merge them into one.", exts: ["pdf"] },
  images: { title: "Images → PDF", kind: "image", hint: "Put the images in page order, then combine them into one PDF.", exts: IMAGE_EXTS },
  split: { title: "Split PDF", kind: "pdf", hint: "Choose pages, e.g. 1-3, 5, 8-. Each range becomes its own PDF. Leave it empty for one PDF per page.", exts: ["pdf"] },
};

let idSeq = 0;
const newId = () => `t${++idSeq}`;

export function Tools() {
  const [tool, setTool] = useState<ToolId>("merge");
  const [lists, setLists] = useState<Record<ToolId, ToolItem[]>>({ merge: [], images: [], split: [] });
  const [ranges, setRanges] = useState("");
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [hovering, setHovering] = useState(false);
  const t = TOOLS[tool];
  const items = lists[tool];
  const toolRef = useRef(tool);
  toolRef.current = tool;
  const listsRef = useRef(lists);
  listsRef.current = lists;

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    const id = toolRef.current;
    try {
      const model = await ipc.inspect(paths);
      // Split takes one PDF: the first one dropped replaces the current choice.
      const pdfs = model.files.filter((f) => f.kind === "pdf");
      const files = id === "split" ? [...pdfs.slice(0, 1), ...model.files.filter((f) => f.kind !== "pdf")] : model.files;
      const base = id === "split" ? [] : listsRef.current[id];
      const { items: next, rejected } = accept(files, TOOLS[id].kind, base, newId);
      const msg = rejection(rejected, TOOLS[id].kind);
      const extra = id === "split" && pdfs.length > 1 ? "Using the first PDF; the others were ignored." : null;
      setNote(msg ? { text: msg, error: true } : extra ? { text: extra, error: false } : null);
      // Split with no PDF in the drop or pick: keep the current item.
      if (id === "split" && pdfs.length === 0) return;
      setLists((all) => ({ ...all, [id]: next }));
    } catch (e) {
      setNote({ text: String(e), error: true });
    }
  }

  useEffect(
    () =>
      router.subscribe("tools", (ev) => {
        if (ev.type === "enter" || ev.type === "over") setHovering(true);
        else if (ev.type === "leave") setHovering(false);
        else {
          setHovering(false);
          void add(ev.paths);
        }
      }),
    // `add` only reads refs and stable setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  async function browse() {
    try {
      const picked = await open({ multiple: tool !== "split", filters: [{ name: t.title, extensions: t.exts }] });
      const paths = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
      await add(paths);
    } catch (e) {
      setNote({ text: String(e), error: true });
    }
  }

  const split = tool === "split";
  const pdf = split ? items[0] : null;
  const check = split && ranges.trim() !== "" ? parseRanges(ranges, pdf?.pages ?? null) : null;
  const canRun = !busy && (split ? !!pdf && (check == null || check.ok) : items.length >= 2);

  async function run() {
    setBusy(true);
    try {
      if (split && pdf) {
        if (ranges.trim() === "") await ipc.startJobs("pdf-split", [pdf.path]);
        else await ipc.startJobs("pdf-extract", [pdf.path], { ranges });
      } else {
        await ipc.startJobs(tool === "merge" ? "pdf-merge" : "image-combine-pdf", items.map((i) => i.path));
      }
      setLists((all) => ({ ...all, [tool]: [] }));
      setRanges("");
      setNote({ text: "Started. Follow it in Activity.", error: false });
    } catch (e) {
      setNote({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={hovering ? "tools drop-hover" : "tools"} aria-label="Tools">
      <div className="segmented" role="group" aria-label="Tool">
        {(Object.keys(TOOLS) as ToolId[]).map((id) => (
          <button key={id} aria-pressed={tool === id} className={tool === id ? "nav active" : "nav"}
            onClick={() => { setTool(id); setNote(null); }}>
            {TOOLS[id].title}
          </button>
        ))}
      </div>
      <p className="muted">{t.hint}</p>
      <div className="dropzone">
        {hovering ? "Release to add" : split ? "Drop a PDF here, or" : "Drop files here, or"}{" "}
        <button onClick={browse}>{split ? "choose a PDF" : "add files"}</button>
      </div>
      {note && <p className={note.error ? "error" : "muted"} role={note.error ? "alert" : "status"}>{note.text}</p>}
      {!split && items.length > 0 && (
        <SortableList items={items} onChange={(next) => setLists((all) => ({ ...all, [tool]: next }))} />
      )}
      {split && pdf && (
        <div className="split">
          <div className="row">
            <div>
              <strong>{pdf.name}</strong>
              <div className="muted">{pdf.pages ? `${pdf.pages} pages` : "Page count unknown until qpdf is installed"}</div>
            </div>
          </div>
          <label className="field">
            <span>Pages</span>
            <input type="text" value={ranges} placeholder="1-3, 5, 8-" onChange={(e) => setRanges(e.target.value)}
              aria-invalid={check != null && !check.ok} aria-describedby="range-help" />
          </label>
          <p id="range-help" className={check && !check.ok ? "error" : "muted"}>
            {check && !check.ok ? check.error : check?.ok ? `${check.ranges.length} PDF${check.ranges.length > 1 ? "s" : ""}` : "Empty: one PDF per page"}
          </p>
        </div>
      )}
      <button className="run" disabled={!canRun} onClick={run}>
        {split ? "Split" : tool === "merge" ? "Merge" : "Make PDF"}
      </button>
    </section>
  );
}
