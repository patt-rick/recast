import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { blockReload } from "../shared/noReload";
import { PadApp } from "./PadApp";
import "./padWindow.css";
import "./pad.css";

blockReload();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <PadApp />
  </StrictMode>,
);
