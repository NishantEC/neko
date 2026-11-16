import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { watchForNewBuild } from "./api";
import { initTheme } from "./theme";
import { embedded } from "./shell/embed";
import "./styles/globals.css";
import "./themes/builtin.generated.css";
import "./styles/sky-materials.css";
import "./styles/app-sheet.css";

// The theme is already on <html> (public/theme-boot.js ran before first paint, the daemon's CSP
// forbids an inline script); this keeps it current and talks to the daemon.
initTheme();
// Embed mode: a transparent page (styles/globals.css), so the host's panel shows through.
if (embedded) document.documentElement.setAttribute("data-embed", "");

// An agent that rebuilds the app (`pnpm watch`) sees the change without a click.
watchForNewBuild();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
