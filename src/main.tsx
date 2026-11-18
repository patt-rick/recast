import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./companion/App";
import "./companion/companion.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
