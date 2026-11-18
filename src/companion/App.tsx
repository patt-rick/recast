import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Activity } from "./Activity";
import { router } from "./dragRouter";
import { Library } from "./Library";
import { SettingsView } from "./Settings";
import { Tools } from "./Tools";

const SECTIONS = [
  { id: "activity", label: "Activity" },
  { id: "tools", label: "Tools" },
  { id: "library", label: "Library" },
  { id: "settings", label: "Settings" },
] as const;
type Section = (typeof SECTIONS)[number]["id"];

export function App() {
  const [section, setSection] = useState<Section>("activity");
  const sectionRef = useRef(section);
  sectionRef.current = section;

  // The one drag-drop listener; the router hands events to the section that should get them.
  useEffect(() => {
    const off = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const t = router.targetFor(sectionRef.current);
      if (p.type === "drop") {
        if (t === "activity") setSection("activity"); // the ring lives in Activity
        router.route(t, { type: "drop", paths: p.paths });
      } else {
        router.route(t, { type: p.type });
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, []);

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Sections">
        <div className="brand">
          <img src="/icon.svg" alt="" width={22} height={22} /> Recast
        </div>
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            className={s.id === section ? "nav active" : "nav"}
            aria-current={s.id === section ? "page" : undefined}
            onClick={() => setSection(s.id)}
          >
            {s.label}
          </button>
        ))}
      </nav>
      {/* All views stay mounted so Activity keeps receiving job events. */}
      <main className="content" hidden={section !== "activity"}>
        <Activity />
      </main>
      <main className="content" hidden={section !== "tools"}>
        <Tools />
      </main>
      <main className="content" hidden={section !== "library"}>
        <Library />
      </main>
      <main className="content" hidden={section !== "settings"}>
        <SettingsView />
      </main>
    </div>
  );
}
