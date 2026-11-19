import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { blockReload } from "./shared/noReload";
import { App } from "./companion/App";
import "./companion/companion.css";
import "./pad/pad.css";

blockReload();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
