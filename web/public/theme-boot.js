// Runs before first paint (a classic script in <head>: the daemon's CSP allows
// an external file and forbids an inline one). It reads what src/theme.ts left
// in localStorage and sets the attributes the stylesheet keys on, so a reload
// never shows the wrong theme for a frame. Keep it small and import nothing;
// scripts/verify/themes.mjs checks it against theme.ts.
(function () {
  try {
    var root = document.documentElement;
    var store = window.localStorage;
    var choice = null;
    try { choice = JSON.parse(store.getItem("neko-appearance")); } catch (e) { /* none yet */ }
    if (!choice || !choice.mode) {
      // The old single setting: system, light, dark or glass. Mapped once; the
      // page offers it to the daemon as the starting point.
      var legacy = store.getItem("neko-theme");
      choice = { mode: legacy === "light" || legacy === "dark" ? legacy : legacy === "glass" ? "dark" : "system", light: "neutral-light", dark: legacy === "glass" ? "glass" : "neutral" };
      if (legacy) {
        store.setItem("neko-appearance", JSON.stringify(choice));
        store.setItem("neko-appearance-seed", "1");
        store.removeItem("neko-theme");
      }
    }
    var forced = new URLSearchParams(location.search).get("theme");
    if (forced === "light" || forced === "dark") choice.mode = forced;
    else if (forced === "glass") { choice.mode = "dark"; choice.dark = "glass"; }
    var dark = choice.mode === "dark" || (choice.mode !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    var appearance = dark ? "dark" : "light";
    var id = dark ? choice.dark : choice.light;
    var cached = null;
    try { cached = (JSON.parse(store.getItem("neko-theme-boot")) || {})[appearance]; } catch (e) { /* none yet */ }
    var backdrop = false;
    if (cached && cached.id === id) {
      backdrop = !!cached.backdrop;
      if (cached.css) {
        var style = document.createElement("style");
        style.id = "neko-user-themes";
        style.textContent = cached.css;
        document.head.appendChild(style);
      }
      var meta = document.querySelector('meta[name="theme-color"]');
      if (meta && cached.bg) meta.setAttribute("content", cached.bg);
    }
    root.setAttribute("data-neko-theme", id);
    root.setAttribute("data-appearance", appearance);
    root.classList.remove("dark", "light");
    root.classList.add(appearance);
    if (backdrop) root.setAttribute("data-backdrop", "");
    root.style.colorScheme = appearance;
  } catch (e) {
    /* storage is blocked: index.html's own defaults stand */
  }
})();
