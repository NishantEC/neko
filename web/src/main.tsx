import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { watchForNewBuild } from "./api";
import "./styles.css";

// An agent that rebuilds the app (`pnpm watch`) sees the change without a click.
watchForNewBuild();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
