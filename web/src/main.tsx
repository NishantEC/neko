import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { watchForNewBuild } from "./api";
import { initTheme } from "./theme";
import "./styles/globals.css";
import "./themes/builtin.generated.css";
import "./styles/sky-materials.css";
import "./styles/app-sheet.css";

// The theme is already on <html> (public/theme-boot.js ran before first paint, the daemon's CSP
// forbids an inline script); this keeps it current and talks to the daemon.
initTheme();

// An agent that rebuilds the app (`pnpm watch`) sees the change without a click.
watchForNewBuild();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
