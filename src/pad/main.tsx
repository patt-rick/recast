import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { PadApp } from "./PadApp";
import "./pad.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <PadApp />
  </StrictMode>,
);
