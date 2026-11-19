import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { blockReload } from "../shared/noReload";
import { Widget } from "./Widget";
import "./widgets.css";

blockReload();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Widget />
  </StrictMode>,
);
