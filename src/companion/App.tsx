import { useState } from "react";
import { Activity } from "./Activity";
import { Library } from "./Library";
import { SettingsView } from "./Settings";

const SECTIONS = [
  { id: "activity", label: "Activity" },
  { id: "library", label: "Library" },
  { id: "settings", label: "Settings" },
] as const;
type Section = (typeof SECTIONS)[number]["id"];

export function App() {
  const [section, setSection] = useState<Section>("activity");
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
        <Activity onDrop={() => setSection("activity")} />
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
