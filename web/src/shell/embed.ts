// Embed mode: the web UI as a piece of another window. The Rune client puts a web view in its
// context panel and loads this app's own route with `embed=1` in the address (docs/knowledge/
// rune-p3b-the-web-panel.md):
//
//   #/artifact/<id>/<n>?embed=1&theme=dark     one artifact and its viewer
//   #/my-plugin:board?embed=1&theme=light      a plugin's page
//
// The flag is read from the hash query (the daemon's sign-in redirect keeps the fragment and drops
// the real query) and, as a convenience, from `?embed=1`. It is read once, when the page loads:
// a page that navigates inside the view keeps its embed mode. Without the flag nothing here does
// anything, and the app is exactly what it was.
function params(text: string): URLSearchParams {
  try {
    return new URLSearchParams(text);
  } catch {
    return new URLSearchParams();
  }
}

/** The query of the hash (`#/route?a=b`), the part the router ignores. */
export function hashQuery(hash: string = location.hash): URLSearchParams {
  return params(hash.split("?")[1] ?? "");
}

/** Whether this page was opened in embed mode. */
export const embedded: boolean = (() => {
  try {
    return hashQuery().get("embed") === "1" || params(location.search).get("embed") === "1";
  } catch {
    return false;
  }
})();

/** `light` or `dark` when the address asks for one (the host window's appearance), else null. */
export function embedTheme(): "light" | "dark" | null {
  const value = hashQuery().get("theme") ?? params(location.search).get("theme");
  return value === "light" || value === "dark" ? value : null;
}
