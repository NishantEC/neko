// Setup, Packs (docs/knowledge/packs.md): the page starts empty ("No packs installed."), Add pack lists the
// bundled catalog (just a list), a folder pack previews with its permissions and what it runs, Install puts it
// in the list with the amber "needs setup" glyph (its one requirement is a program that is not installed, so no
// real program ever runs), Check health works, the detail shows the install hint with a Copy button, and Remove
// asks first (Cancel sends nothing) and then removes. Install always carries the digest the preview returned
// (no "approve"); an Update shows its own preview first (plain "Update", or "Update needs approval" with the
// changes when the pack can do more than before); a pack whose installed files were edited reads "Changed on
// disk" and is approved again through Review. A refusal shows the daemon's words inside the dialog.
//
//   NEKO_WEB_ONLY=packs node scripts/web-verify.mjs   (see scripts/verify/README.md)
//
// The harness is started with NEKO_PACKS_DIR=<repo>/packs, so the catalog is the repository's bundled packs.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const NAME = "packs-verify-demo";
const TITLE = "Packs Verify Demo";
const MISSING = "neko-verify-missing-tool";
const HINT = "brew install neko-verify-missing-tool";

const MANIFEST = `name = "${NAME}"
title = "${TITLE}"
version = "0.1.0"
description = "A made-up pack for the web checks. It needs a program that is not installed."
homepage = "https://example.invalid/packs-verify-demo"
licence = "MIT"

[requires]
binaries = [{ name = "${MISSING}", hint = "${HINT}" }]

[[skills]]
path = "skills/demo-skill"

[health]
command = ["${MISSING}", "doctor"]

[permissions]
summary = [
  "Reads files in the folders you open it from",
  "Shows a made-up status in the Neko column",
]
`;

const SKILL = `---
name: demo-skill
description: A made-up skill used only by the web checks.
---

Say hello.
`;

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const base = `http://127.0.0.1:${PORT}`;
  const api = (body) => page.evaluate(async (b) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) })).json(), body);
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  const folder = path.join(DATA, "fake-pack");
  mkdirSync(path.join(folder, "skills/demo-skill"), { recursive: true });
  writeFileSync(path.join(folder, "pack.toml"), MANIFEST);
  writeFileSync(path.join(folder, "skills/demo-skill/SKILL.md"), SKILL);

  // Every request a Packs action sends, to prove nothing is sent before a yes.
  const sent = [];
  page.on("request", (r) => {
    if (!r.url().endsWith("/api/request") || r.method() !== "POST") return;
    const body = r.postData() ?? "";
    if (body.includes('"Packs"')) sent.push(body);
  });
  const sentWith = (word) => sent.filter((b) => b.includes(word)).length;

  // ---- empty -----------------------------------------------------------------------------------
  await page.goto(`${base}/#/packs`);
  await page.locator("[data-slot='packs-empty']").waitFor({ timeout: 15000 });
  check((await page.locator("[data-slot='packs-empty']").innerText()).trim() === "No packs installed.", "an empty list is one quiet line");
  check((await page.locator("[data-slot='pack']").count()) === 0, "no pack rows yet");
  const listed = (await api({ Packs: "List" })).Packs;
  check(listed.installed.length === 0, "the daemon lists no installed packs");
  check(listed.catalog.length >= 1 && listed.catalog.some((c) => c.info.name === "spaceo"), "the daemon lists the bundled catalog");
  await shot("packs-empty");
  await a11y("packs (empty)");

  // ---- Add pack: the catalog is a list ----------------------------------------------------------
  await page.getByRole("button", { name: "Add pack" }).first().click();
  const dialog = page.locator("[data-slot='add-pack']");
  await dialog.waitFor({ timeout: 8000 });
  const entries = dialog.locator("[data-slot='catalog-entry']");
  check((await entries.count()) === listed.catalog.length, "the Catalog tab lists every bundled pack");
  const first = await entries.first().innerText();
  check(/\d+\.\d+\.\d+/.test(first) && /Add/.test(first), `a catalog row has its title, version and an Add button (${first.replace(/\n/g, " | ")})`);
  const dialogText = (await dialog.innerText()).toLowerCase();
  check(!/recommend|suggest|popular|featured|try /.test(dialogText), "the catalog nudges nothing");
  await shot("packs-add-catalog");

  // ---- Preview of the fake folder ---------------------------------------------------------------
  await dialog.getByRole("tab", { name: "From folder or URL" }).click();
  const field = dialog.getByRole("textbox", { name: "Folder or URL" });
  await field.fill(path.join(DATA, "no-such-pack"));
  await dialog.getByRole("button", { name: "Preview" }).click();
  await dialog.locator("[data-slot='error-note']").waitFor({ timeout: 8000 });
  const refusal = (await dialog.locator("[data-slot='error-note']").innerText()).trim();
  check(refusal.length > 12, `a refusal shows the daemon's words inside the dialog (${refusal.replace(/\n/g, " | ").slice(0, 100)})`);
  check(await dialog.getByRole("button", { name: "Preview" }).isEnabled(), "and the buttons are back");
  check(sentWith('"Install"') === 0, "nothing was installed by looking");

  await field.fill(folder);
  await dialog.getByRole("button", { name: "Preview" }).click();
  const preview = dialog.locator("[data-slot='pack-preview']");
  await preview.waitFor({ timeout: 8000 });
  const pv = await preview.innerText();
  check(pv.includes(TITLE) && pv.includes("0.1.0") && pv.includes("MIT"), "the preview shows title, version and licence");
  check(pv.includes("Reads files in the folders you open it from") && pv.includes("Shows a made-up status in the Neko column"), "the preview lists the permissions");
  check((await preview.locator("[data-slot='pack-runs'] code").allInnerTexts()).some((t) => t.includes(`${MISSING} doctor`)), "What it runs shows the health command in monospace");
  check(/Skills/.test(pv) && pv.includes("demo-skill"), "the preview lists the skill it adds");
  check(/switched off/.test(pv) && /grant/i.test(pv), "it says routines start off and tools are granted to nobody");
  check((await preview.locator("[data-slot='pack-requirement'][data-found='false']").count()) === 1 && pv.includes(MISSING) && pv.includes(HINT), "the missing requirement shows with its hint");
  check((await preview.locator("[data-slot='pack-env']").count()) === 0, "no value fields when the pack asks for none");
  await shot("packs-add-preview");
  await a11y("packs (preview)");

  // ---- Install ----------------------------------------------------------------------------------
  await preview.getByRole("button", { name: "Install", exact: true }).click();
  await dialog.waitFor({ state: "detached", timeout: 15000 });
  const row = page.locator(`[data-slot='pack'][data-pack='${NAME}']`);
  await row.waitFor({ timeout: 8000 });
  check((await row.getAttribute("data-status")) === "NeedsSetup", "the installed pack needs setup (its program is missing)");
  check((await row.locator("[data-slot='status-glyph'][data-status='inProgress']").count()) === 1, "and shows the amber half-filled glyph");
  const rowText = await row.innerText();
  check(rowText.includes(TITLE) && rowText.includes("0.1.0") && rowText.includes("fake-pack"), `the row has title, version and source (${rowText.replace(/\n/g, " | ")})`);
  check((await page.locator("[data-slot='packs-empty']").count()) === 0, "the empty line is gone");
  const install = sent.find((b) => b.includes('"Install"')) ?? "";
  check(/"digest":"[0-9a-f]{8,}"/.test(install) && !install.includes('"approve"') && !install.includes('"env"'), "Install carries the previewed digest, no approve and no env when none is asked for");
  await shot("packs-list");
  await a11y("packs (list)");

  // ---- Check health -----------------------------------------------------------------------------
  const before = sentWith('"Health"');
  await row.getByRole("button", { name: `Check health of ${TITLE}` }).click();
  await sleep(1500);
  check(sentWith('"Health"') === before + 1, "Check health sends one Health request");
  check((await row.getAttribute("data-status")) === "NeedsSetup", "a missing program still reads as needs setup");

  // ---- Detail -----------------------------------------------------------------------------------
  await row.getByRole("button", { name: /Open details/ }).click();
  const detail = page.locator("[data-slot='pack-detail']");
  await detail.waitFor({ timeout: 8000 });
  const dt = await detail.innerText();
  check(dt.includes("A made-up pack for the web checks"), "the detail shows the description");
  check(dt.includes("Reads files in the folders you open it from"), "and the permissions");
  check(dt.includes("demo-skill"), "and its contents");
  check(dt.includes(MISSING) && dt.includes("Missing") && dt.includes(HINT), "and the requirement with its install hint");
  check((await detail.locator("[data-slot='pack-requirement'] code").first().innerText()).includes(HINT), "the hint is monospace");
  await detail.getByRole("button", { name: /Copy the command/ }).click();
  await detail.locator("[data-slot='pack-health']").waitFor();
  const health = await detail.locator("[data-slot='pack-health']").innerText();
  check(/Last health check/.test(health) && health.includes(MISSING), `the last health output shows (${health.replace(/\n/g, " | ").slice(0, 120)})`);
  await shot("packs-detail");
  await a11y("packs (detail)");
  await page.keyboard.press("Escape");
  await detail.waitFor({ state: "detached", timeout: 5000 });

  // ---- Update: a plain one, then one that needs approval -----------------------------------------
  const bump = (version, extra) => writeFileSync(path.join(folder, "pack.toml"), MANIFEST.replace('version = "0.1.0"', `version = "${version}"`).replace(/\n\[permissions\]\nsummary = \[\n/, `\n[permissions]\nsummary = [\n${extra}`));
  bump("0.2.0", "");
  await page.reload();
  await row.waitFor({ timeout: 15000 });
  const updateButton = row.getByRole("button", { name: `Update ${TITLE} to 0.2.0` });
  await updateButton.waitFor({ timeout: 15000 });
  check(true, "a newer version in the source shows Update to 0.2.0 on the row");
  const previewsBefore = sentWith('"Preview"');
  await updateButton.click();
  const review = page.locator("[data-slot='review-pack']");
  await review.waitFor({ timeout: 8000 });
  check(sentWith('"Preview"') === previewsBefore + 1 && sentWith('"Update"') === 0, "Update previews first and sends nothing else yet");
  check(!/needs approval/i.test(await review.innerText()) && (await review.getByRole("button", { name: "Update", exact: true }).count()) === 1, "an update that changes no power is a plain Update");
  await shot("packs-update-plain");
  await review.getByRole("button", { name: "Cancel" }).click();
  await review.waitFor({ state: "detached", timeout: 5000 });
  check(sentWith('"Update"') === 0, "Cancel sends no Update");
  await row.getByRole("button", { name: `Update ${TITLE} to 0.2.0` }).click();
  await review.waitFor({ timeout: 8000 });
  await review.getByRole("button", { name: "Update", exact: true }).click();
  await review.waitFor({ state: "detached", timeout: 15000 });
  const updateBody = sent.find((b) => b.includes('"Update"')) ?? "";
  check(/"digest":"[0-9a-f]{8,}"/.test(updateBody), "Update sends the previewed digest");
  await sleep(500);
  check((await row.innerText()).includes("0.2.0"), "the row now shows 0.2.0");

  bump("0.3.0", '  "Writes a made-up file on your desktop",\n');
  await page.reload();
  await row.getByRole("button", { name: `Update ${TITLE} to 0.3.0` }).waitFor({ timeout: 15000 });
  await row.getByRole("button", { name: `Update ${TITLE} to 0.3.0` }).click();
  await review.waitFor({ timeout: 8000 });
  const rt = await review.innerText();
  check(/Update needs approval/.test(rt) && rt.includes("Writes a made-up file on your desktop"), "an update that adds a permission says it needs approval and lists the change");
  check((await review.getByRole("button", { name: "Approve and update" }).count()) === 1, "with an Approve and update button");
  await shot("packs-update-approval");
  await review.getByRole("button", { name: "Approve and update" }).click();
  await review.waitFor({ state: "detached", timeout: 15000 });
  await sleep(500);
  check((await row.innerText()).includes("0.3.0"), "approving installs 0.3.0");

  // ---- Changed on disk -------------------------------------------------------------------------
  const installedToml = path.join(DATA, "packs", NAME, "pack.toml");
  writeFileSync(installedToml, readFileSync(installedToml, "utf8").replace("Reads files in the folders you open it from", "Reads every file on this Mac"));
  await page.reload();
  const changedRow = page.locator(`[data-slot='pack'][data-pack='${NAME}']`);
  await changedRow.waitFor({ timeout: 15000 });
  await page.waitForFunction((n) => document.querySelector(`[data-pack='${n}']`)?.getAttribute("data-status") === "Changed", NAME, { timeout: 15000 });
  check(true, "editing the installed files reads as Changed on disk");
  check((await changedRow.innerText()).length > 0 && (await changedRow.getByRole("button", { name: `Review ${TITLE}` }).count()) === 1, "with a Review button on the row");
  await shot("packs-changed");
  await changedRow.getByRole("button", { name: `Review ${TITLE}` }).click();
  await review.waitFor({ timeout: 8000 });
  check(/Review/.test(await review.locator("h2, [data-slot='dialog-title']").first().innerText()) && (await review.innerText()).includes("Reads every file on this Mac"), "Review shows what the installed copy says now");
  await review.getByRole("button", { name: "Approve", exact: true }).click();
  await review.waitFor({ state: "detached", timeout: 15000 });
  await sleep(500);
  check((await changedRow.getAttribute("data-status")) !== "Changed", "approving brings it back from Changed on disk");
  await changedRow.getByRole("button", { name: /Open details/ }).click();
  await detail.waitFor({ timeout: 8000 });
  await detail.getByRole("button", { name: /Remove/ }).click();

  // ---- Remove asks first ------------------------------------------------------------------------
  const confirmDialog = page.getByRole("alertdialog");
  await confirmDialog.waitFor({ timeout: 8000 });
  check(/Remove/.test(await confirmDialog.innerText()), "Remove opens a confirmation");
  check((await confirmDialog.getByRole("button", { name: "Remove", exact: true }).count()) === 1, "with a Remove button");
  await shot("packs-confirm");
  const removesBefore = sentWith('"Remove"');
  await confirmDialog.getByRole("button", { name: "Cancel" }).click();
  await confirmDialog.waitFor({ state: "detached", timeout: 5000 });
  await sleep(400);
  check(sentWith('"Remove"') === removesBefore, "Cancel sends nothing");
  check((await page.locator(`[data-slot='pack'][data-pack='${NAME}']`).count()) === 1, "and the pack is still listed");

  await page.keyboard.press("Escape");
  await detail.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
  await page.locator(`[data-slot='pack'][data-pack='${NAME}']`).getByRole("button", { name: `Actions for ${TITLE}` }).click();
  await page.getByRole("menuitem", { name: /Remove/ }).click();
  const confirm2 = page.getByRole("alertdialog");
  await confirm2.waitFor({ timeout: 8000 });
  await confirm2.getByRole("button", { name: "Remove", exact: true }).click();
  await page.locator("[data-slot='packs-empty']").waitFor({ timeout: 15000 });
  check(sentWith('"Remove"') === removesBefore + 1, "yes sends exactly one Remove");
  check((await api({ Packs: "List" })).Packs.installed.length === 0, "the daemon no longer lists it");
  check((await page.locator("[data-slot='pack']").count()) === 0, "the row is gone");
  await confirm2.waitFor({ state: "detached", timeout: 5000 });
  await a11y("packs (after remove)");
}
