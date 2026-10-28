// What "Make me a theme" tells Neko. One constant, shown to the person in the
// dialog before it is sent. {{REQUEST}} and {{THEMES_DIR}} are filled in at send
// time. It must stay under the daemon's message limit (4096 bytes, request
// included). The schema here is the one in docs/design/themes.md, in
// themes/derive.js (validateTheme) and in crates/neko-core/src/appearance.rs:
// change them together.
export const THEME_PROMPT = `Make me a Neko theme: {{REQUEST}}

Write ONE file {{THEMES_DIR}}/<id>.json (create the folder if it is missing). <id> is lowercase letters, digits and hyphens (1-48) and must equal "id" inside. Neko loads it within seconds and lists it in Settings, Appearance; do not edit Neko's source for this. Do not overwrite another theme file of mine.

The file is exactly this shape (no comments in the real file; any other key is rejected and the file is skipped with a warning):
{
  "id": "ocean",                 file name without .json
  "name": "Ocean",               1-60 characters
  "appearance": "dark",          "light" or "dark": which picker lists it
  "family": "Mine",              optional label, 1-40 characters
  "colors": {                    hex only: #rgb, #rrggbb or #rrggbbaa
    "background": "#0b1f2a",     required, the page
    "foreground": "#d7e9f0",     required, body text
    "primary": "#3ab0d0",        required, buttons and focus ring
    optional, derived when left out: "card" "popover" "sidebar" (surfaces), "strong" (headings), "mutedForeground" (secondary text), "border" (hairline hue), "primaryForeground", "destructive" "success" "warning" "info", "bubble" "bubbleForeground" (my chat messages)
  },
  "backdrop": { "blur": 22, "base": ["#0b1424", "#101a30"], "glows": [{ "color": "#483e90", "alpha": 0.5, "x": 12, "y": 0, "width": 70, "height": 60 }] }
}
"backdrop" is optional and makes frosted glass: blur 0-60, 1-4 base colours (a gradient), up to 4 glows (alpha 0-1, x and y in percent, width and height 1-200). Surfaces then turn translucent, so keep their colours solid in the file.

Rules: a dark theme has a dark background and light text, a light theme the reverse. Text needs at least 4.5:1 contrast on the surfaces (foreground and mutedForeground on background, card and sidebar); a theme below that still loads but shows a warning chip, so work the ratios out and fix them. No CSS, images, fonts or URLs. To replace a built-in use its id (neutral, neutral-light, glass, catppuccin-latte, catppuccin-mocha, gruvbox-dark, gruvbox-light, solarized-dark, solarized-light, nord, tokyo-night, rose-pine); deleting my file brings the built-in back.

When done, open Settings, Appearance in the browser, check it is listed with no warning, pick it, look at the app, fix what looks wrong, then tell me where to find it.`;

export const THEME_EXAMPLE = `{
  "id": "ocean",
  "name": "Ocean",
  "appearance": "dark",
  "colors": { "background": "#0b1f2a", "foreground": "#d7e9f0", "primary": "#3ab0d0" }
}`;

const LIMIT_BYTES = 4096;
const bytes = (text: string) => new TextEncoder().encode(text).length;
const FALLBACK_DIR = "<data dir>/themes";

function fill(request: string, dir: string | null): string {
  // Replacer functions, so a $ in the request or a path is not read as a replacement pattern.
  return THEME_PROMPT.replaceAll("{{THEMES_DIR}}", () => dir ?? FALLBACK_DIR).replaceAll("{{REQUEST}}", () => request);
}

/** How many bytes the request itself may use once the rest of the prompt is in. */
export const themeRequestBudget = (dir: string | null) => LIMIT_BYTES - bytes(fill("", dir));

export const themePrompt = (request: string, dir: string | null) => fill(request.trim(), dir);
