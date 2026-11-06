// The canvas beside the chat: an artifact saved while a Home reply runs gets a
// card on that reply and opens on the canvas; the frame is sandboxed (no
// cookie, no storage, no API); comment mode turns a click into a reference
// chip in the composer; a second version replaces the first; the card's
// "Open in a tab" gives the artifact a tab of its own. There is no Canvas
// gallery page any more (#/canvas is a missing route).
//
//   NEKO_WEB_ONLY=canvas node scripts/web-verify.mjs
import { writeFileSync } from "node:fs";
import path from "node:path";

const DESIGN = (heading) => `<!doctype html><html><head><meta charset="utf-8"><title>Onboarding</title>
<style>body{font:16px/1.5 system-ui,sans-serif;margin:0;padding:48px;background:#f6f7fb;color:#111}h1{font-size:32px;margin:0 0 12px}button{font:inherit;padding:10px 18px;border-radius:10px;border:0;background:#2f6feb;color:#fff}</style>
</head><body><main><h1 id="headline">${heading}</h1><p>Three steps and you are in.</p><button type="button">Get started</button></main>
<script>document.body.dataset.ran = "yes";</script></body></html>`;

export default async function ({ page, shot, a11y, check, PORT, DATA }) {
  const home = `http://127.0.0.1:${PORT}/#/`;
  const file = path.join(DATA, "canvas-onboarding.html");
  writeFileSync(file, DESIGN("Welcome aboard"));
  await page.goto(home);
  await page.waitForSelector("[data-slot='neko-turn']", { timeout: 20000 });

  const save = (artifactId) => page.evaluate(async ([file, artifactId]) => {
    const response = await fetch("/api/request", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ SaveArtifact: { title: "Onboarding screen", file, workspace_id: null, artifact_id: artifactId, note: artifactId ? "Bigger headline" : "First pass" } }),
    });
    return response.json();
  }, [file, artifactId]);
  const first = await save(null);
  const saved = first?.ArtifactSaved;
  check(!!saved && saved.version === 1, "SaveArtifact keeps the file as version 1");
  const id = saved?.artifact?.id;

  // The reply that was running gets the card; the canvas opens beside the chat.
  await page.waitForSelector(`[data-slot='artifact-panel'][data-artifact='${id}']`, { timeout: 10000 });
  check(true, "the canvas opens beside the conversation");
  check(!!saved?.artifact?.message_id, "the artifact is linked to the reply that was running");
  await page.waitForSelector(`[data-slot='artifact-card'][data-artifact='${id}']`, { timeout: 10000 }).then(() => check(true, "the reply shows the artifact's card"), () => check(false, "the reply shows the artifact's card"));

  const frameEl = page.locator("[data-slot='artifact-panel'] iframe[data-slot='artifact-frame']");
  check((await frameEl.getAttribute("sandbox")) === "allow-scripts", "the frame is sandboxed with scripts only");
  const design = page.frameLocator("[data-slot='artifact-panel'] iframe[data-slot='artifact-frame']");
  await design.locator("#headline").waitFor({ timeout: 10000 });
  check((await design.locator("#headline").innerText()) === "Welcome aboard", "the design renders live");
  const frame = page.frames().find((f) => f.url().includes(`/artifact/${id}/`));
  if (frame) {
    const probe = await frame.evaluate(async () => {
      const out = { ran: document.body.dataset.ran, cookie: "", storage: "" };
      try { out.cookie = document.cookie; } catch { out.cookie = "blocked"; }
      try { localStorage.setItem("x", "1"); out.storage = "open"; } catch { out.storage = "blocked"; }
      return out;
    });
    check(probe.ran === "yes", "the design's own script runs");
    check(probe.cookie === "blocked" || probe.cookie === "", "the design cannot read Neko's cookie");
    check(probe.storage === "blocked", "the design cannot use Neko's storage");
    // Not probed from inside (a blocked fetch is a console error): the header says it.
    const policy = await page.evaluate(async (url) => (await fetch(url)).headers.get("content-security-policy") ?? "", `/artifact/${id}/1`);
    check(policy.includes("connect-src 'none'") && policy.includes("sandbox allow-scripts"), "the design cannot reach Neko's API (connect-src 'none', sandboxed)");
  } else {
    check(false, "found the artifact's frame");
  }
  await shot("canvas-open");

  // Comment mode: a click on the headline becomes a reference chip in the composer.
  await page.locator("[data-slot='artifact-panel'] button[aria-pressed]").click();
  await page.waitForSelector("[data-slot='artifact-frame'][data-commenting='true']", { timeout: 5000 });
  await design.locator("#headline").hover();
  await shot("canvas-commenting");
  await design.locator("#headline").click();
  const chip = page.locator("[data-slot='attachment']", { hasText: "Onboarding screen v1" });
  await chip.waitFor({ timeout: 5000 }).then(() => check(true, "a picked element becomes a chip in the composer"), () => check(false, "a picked element becomes a chip in the composer"));
  check(/Canvas/.test(await chip.innerText().catch(() => "")), "the chip says it is about the canvas");
  await shot("canvas-comment-chip");
  await chip.locator("button").click();

  // A second version replaces the first on the canvas, and the version switcher appears.
  writeFileSync(file, DESIGN("Welcome aboard, bigger"));
  const second = await save(id);
  check(second?.ArtifactSaved?.version === 2, "saving with artifact_id adds version 2");
  await page.waitForSelector(`[data-slot='artifact-panel'][data-version='2']`, { timeout: 10000 }).then(() => check(true, "the canvas follows the new version"), () => check(false, "the canvas follows the new version"));
  await design.locator("#headline", { hasText: "bigger" }).waitFor({ timeout: 10000 }).catch(() => {});
  check((await page.locator("[data-slot='artifact-card']").count()) === 1, "one card per artifact on the reply");
  await shot("canvas-v2");

  // The card opens the artifact in a tab of its own, beside Home's tab.
  await page.locator("[data-slot='artifact-card'] button[title='Open in a tab']").first().click();
  await page.waitForSelector(`[data-slot='artifact-tab'] [data-slot='artifact-panel'][data-artifact='${id}']`, { timeout: 10000 }).then(() => check(true, "Open in a tab shows the artifact in its own tab"), () => check(false, "Open in a tab shows the artifact in its own tab"));
  check((await page.locator("[role=tablist][aria-label='Open tabs'] [role=tab][aria-selected=true]").innerText().catch(() => "")).includes("Onboarding"), "the artifact's tab is selected and named after it");
  await shot("canvas-tab");
  await a11y("artifact tab");
  // The gallery page is gone.
  await page.goto(`http://127.0.0.1:${PORT}/#/canvas`);
  await page.waitForSelector("[data-slot='missing']", { timeout: 10000 }).then(() => check(true, "#/canvas is no longer a page"), () => check(false, "#/canvas is no longer a page"));
  await page.goto(`http://127.0.0.1:${PORT}/#/`);
  await page.locator("button[aria-label='Close the inspector']").click();
}
