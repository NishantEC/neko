// Light and dark follow the system preference unless the owner picked one.
// The choice is a class on <html> (`dark` or `light`) that the tokens in
// styles/globals.css key off. `?theme=light|dark` on the first load forces one;
// scripts/web-verify.mjs instead emulates the system preference, which is what
// the app follows.

export type ThemeChoice = "system" | "light" | "dark";

const KEY = "neko-theme";
const query = matchMedia("(prefers-color-scheme: dark)");

function stored(): ThemeChoice {
  const forced = new URLSearchParams(location.search).get("theme");
  if (forced === "light" || forced === "dark") return forced;
  try {
    const value = localStorage.getItem(KEY);
    if (value === "light" || value === "dark") return value;
  } catch {
    /* storage can be blocked in an embedded view */
  }
  return "system";
}

let choice: ThemeChoice = stored();
const listeners = new Set<() => void>();

function resolved(): "light" | "dark" {
  return choice === "system" ? (query.matches ? "dark" : "light") : choice;
}

function apply() {
  const root = document.documentElement;
  const theme = resolved();
  root.classList.toggle("dark", theme === "dark");
  root.classList.toggle("light", theme === "light");
  root.style.colorScheme = theme;
  listeners.forEach((l) => l());
}

export function initTheme() {
  apply();
  query.addEventListener("change", () => {
    if (choice === "system") apply();
  });
}

export function setTheme(next: ThemeChoice) {
  choice = next;
  try {
    if (next === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, next);
  } catch {
    /* ignore */
  }
  apply();
}

export const getTheme = () => choice;
export const getResolvedTheme = resolved;

export function subscribeTheme(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
