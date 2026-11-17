// Setup, Improvements (docs/knowledge/self-improvement.md): Neko's suggestions after it notices the same trouble
// again. The harness seeds five proposals (web_harness.rs `seed_improve`): imp-open (open skill_patch), imp-applied
// (applied standing_order), imp-helped (helped install_pack), imp-bad (didnt_help code_task) and imp-rejected
// (rejected new_skill). This checks the rows and their glyphs and words, the detail with its evidence, that Undo
// asks first only when the daemon says so (a code task) and not for a standing order, that Reject never asks,
// the Suggestions switch in Settings, and, with the daemon's answer replaced for a moment (page.route), the
// empty state, a suggestion waiting in Skills, and Apply on a pack suggestion opening that pack's preview.
//
//   NEKO_WEB_ONLY=improve node scripts/web-verify.mjs   (see scripts/verify/README.md)
//
// The harness is started with NEKO_PACKS_DIR=<repo>/packs, so `catalog:spaceo` previews for real.
const GLYPH = { open: "inProgress", sent: "inProgress", applied: "done", helped: "done", didnt_help: "failed", rejected: "cancelled" };
const SEEDS = [
  ["imp-open", "open", "Run the type check before reviewing", "Improve a skill"],
  ["imp-applied", "applied", "Say which branch a PR targets", "Standing order"],
  ["imp-helped", "helped", "Install the Linear pack", "Install a pack"],
  ["imp-bad", "didnt_help", "Retry flaky test setup", "Change Neko"],
  ["imp-rejected", "rejected", "A skill for release notes", "New skill"],
];

export default async function ({ page, shot, a11y, check, sleep, PORT }) {
  const base = `http://127.0.0.1:${PORT}`;
  const api = (body) => page.evaluate(async (b) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) })).json(), body);
  await page.setViewportSize({ width: Number(process.env.NEKO_WEB_WIDTH ?? 1440), height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });

  // Every Improve request the page sends, to prove nothing is sent before a yes.
  const sent = [];
  page.on("request", (r) => {
    if (!r.url().endsWith("/api/request") || r.method() !== "POST") return;
    const body = r.postData() ?? "";
    if (body.includes('"Improve"')) sent.push(body);
  });
  const sentWith = (word) => sent.filter((b) => b.includes(word)).length;
  const row = (id) => page.locator(`[data-slot='improvement'][data-id='${id}']`);

  // ---- the list ---------------------------------------------------------------------------------
  await page.goto(`${base}/#/improvements`);
  await page.locator("[data-slot='improvement']").first().waitFor({ timeout: 15000 });
  const listed = (await api({ Improve: "List" })).Improve;
  check(listed.enabled === true, "the daemon says suggestions are on");
  check(listed.proposals.length === SEEDS.length, `the daemon lists the ${SEEDS.length} seeded proposals`);
  check((await page.locator("[data-slot='improvement']").count()) === SEEDS.length, "the page shows one row per proposal");
  const order = await page.locator("[data-slot='improvement']").evaluateAll((els) => els.map((e) => e.getAttribute("data-id")));
  check(JSON.stringify(order) === JSON.stringify(listed.proposals.map((p) => p.id)), "in the daemon's order (newest first)");
  check((await page.locator("[data-slot='improve-off'], [data-slot='improve-note']").count()) === 0, "no off notice and no note while suggestions are on");
  for (const [id, state, title, kind] of SEEDS) {
    const r = row(id);
    check((await r.getAttribute("data-state")) === state, `${id} reads ${state}`);
    check((await r.locator(`[data-slot='status-glyph'][data-status='${GLYPH[state]}']`).count()) === 1, `${id} shows the ${GLYPH[state]} glyph`);
    const text = await r.innerText();
    check(text.includes(title) && text.includes(kind), `${id} has its title and kind (${text.replace(/\n/g, " | ").slice(0, 90)})`);
  }
  check(/Helped/.test(await row("imp-helped").locator("[data-slot='improve-word']").innerText()), "a helped suggestion says Helped");
  check(/Didn.t help/.test(await row("imp-bad").locator("[data-slot='improve-word']").innerText()), "one that didn't help says so");
  check(/Rejected/.test(await row("imp-rejected").locator("[data-slot='improve-word']").innerText()), "a rejected one says Rejected");
  check((await row("imp-open").getByRole("button", { name: /^Apply / }).count()) === 1 && (await row("imp-open").getByRole("button", { name: /^Reject / }).count()) === 1, "the open one has Apply and Reject");
  check((await row("imp-helped").getByRole("button", { name: /^Apply / }).count()) === 0 && (await row("imp-rejected").getByRole("button", { name: /^Reject / }).count()) === 0, "the others have neither");
  for (const p of listed.proposals) {
    check((await row(p.id).getByRole("button", { name: /^Undo / }).count()) === (p.can_undo ? 1 : 0), `${p.id}: Undo shows exactly when the daemon says can_undo (${p.can_undo})`);
  }
  await shot("improve-list");
  await a11y("improvements (list)");

  // ---- the detail, opened at its evidence ----------------------------------------------------------
  const open = listed.proposals.find((p) => p.id === "imp-open");
  await row("imp-open").getByRole("button", { name: /^Open evidence for / }).click();
  const detail = page.locator("[data-slot='improve-detail']");
  await detail.waitFor({ timeout: 8000 });
  const dt = await detail.innerText();
  check(dt.includes(open.why.slice(0, 30)), "the detail shows why");
  check((await detail.locator("[data-slot='improve-diff']").count()) === 1 && (await detail.locator("[data-slot='improve-diff'] [data-diff='add']").count()) >= 1, "a skill patch shows its change as a diff");
  const ev = detail.locator("[data-slot='improve-evidence']");
  check((await ev.locator("code").count()) === 1 && (await ev.innerText()).includes(open.evidence.trim().split("\n")[0].slice(0, 20)), "the evidence is a monospace block");
  await sleep(300);
  check(await ev.evaluate((el) => { const r = el.getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; }), "Open evidence scrolls the detail to the evidence");
  await shot("improve-detail");
  await a11y("improvements (detail)");
  await page.keyboard.press("Escape");
  await detail.waitFor({ state: "detached", timeout: 5000 });

  const applied = listed.proposals.find((p) => p.id === "imp-applied");
  await row("imp-applied").getByRole("button", { name: /Open details/ }).click();
  await detail.waitFor({ timeout: 8000 });
  const at = await detail.innerText();
  check(at.includes(`Fired ${applied.before} time`) && at.includes(`${applied.after} since`), `an applied one says how often it fired before and since (before ${applied.before}, after ${applied.after})`);
  check((await detail.locator("[data-slot='improve-change']").innerText()).includes(applied.change.slice(0, 20)) && (await detail.locator("[data-slot='improve-diff']").count()) === 0, "a standing order shows its change as text");
  check(applied.history.length === 0 || (await detail.locator("[data-slot='improve-history'] li").count()) === applied.history.length, "the history lists every event");
  await page.keyboard.press("Escape");
  await detail.waitFor({ state: "detached", timeout: 5000 });

  await row("imp-helped").getByRole("button", { name: /Open details/ }).click();
  await detail.waitFor({ timeout: 8000 });
  const pt = await detail.locator("[data-slot='improve-change']").innerText();
  check(/pack/i.test(pt) && /nothing installs by itself/i.test(pt), "a pack suggestion says it opens the preview and installs nothing itself");
  await page.keyboard.press("Escape");
  await detail.waitFor({ state: "detached", timeout: 5000 });

  // ---- Undo: asks only when the daemon says so ---------------------------------------------------------
  const bad = listed.proposals.find((p) => p.id === "imp-bad");
  check(bad.can_undo && bad.undo_confirms, "the code-task suggestion can be undone and asks first");
  await row("imp-bad").getByRole("button", { name: /^Undo / }).click();
  const confirmDialog = page.getByRole("alertdialog");
  await confirmDialog.waitFor({ timeout: 8000 });
  check((await confirmDialog.getByRole("button", { name: "Undo", exact: true }).count()) === 1, "Undo on a code task opens a confirmation");
  await shot("improve-confirm");
  await confirmDialog.getByRole("button", { name: "Cancel" }).click();
  await confirmDialog.waitFor({ state: "detached", timeout: 5000 });
  await sleep(300);
  check(sentWith('"Undo"') === 0, "Cancel sends no Undo");
  check((await row("imp-bad").getAttribute("data-state")) === "didnt_help", "and nothing changed");

  check(applied.can_undo && !applied.undo_confirms, "the standing-order suggestion can be undone without asking");
  await row("imp-applied").getByRole("button", { name: /^Undo / }).click();
  await page.waitForFunction(() => document.querySelector("[data-id='imp-applied']")?.getAttribute("data-state") !== "applied", null, { timeout: 8000 });
  check((await page.getByRole("alertdialog").count()) === 0, "Undo on a standing order opens no confirmation");
  check(sentWith('"Undo"') === 1 && sent.some((b) => b.includes('"Undo"') && b.includes("imp-applied")), "and sends exactly one Undo, for it");
  check((await row("imp-applied").getAttribute("data-state")) === "rejected", "an undone suggestion reads Rejected");
  check((await row("imp-applied").getByRole("button", { name: /^Undo / }).count()) === 0, "with no Undo left");

  // ---- Reject never asks ------------------------------------------------------------------------------------
  await row("imp-open").getByRole("button", { name: /^Reject / }).click();
  await page.waitForFunction(() => document.querySelector("[data-id='imp-open']")?.getAttribute("data-state") === "rejected", null, { timeout: 8000 });
  check((await page.getByRole("alertdialog").count()) === 0, "Reject opens no confirmation");
  check(sentWith('"Reject"') === 1 && sent.some((b) => b.includes('"Reject"') && b.includes("imp-open")), "and sends exactly one Reject, for it");
  check((await row("imp-open").getByRole("button", { name: /^(Apply|Reject) / }).count()) === 0, "Apply and Reject are gone from it");
  check((await api({ Improve: "List" })).Improve.proposals.find((p) => p.id === "imp-open").state === "rejected", "the daemon agrees");
  await shot("improve-after");

  // ---- Settings: the Suggestions switch ----------------------------------------------------------------------------
  await page.goto(`${base}/#/settings?section=ai`);
  const row2 = page.locator("[data-slot='setting-row'][data-setting='suggestions']");
  await row2.waitFor({ timeout: 15000 });
  const switchEl = row2.getByRole("switch", { name: "Suggestions" });
  await page.waitForFunction(() => document.querySelector("[data-setting='suggestions'] [role='switch']"), null, { timeout: 8000 });
  check((await row2.innerText()).includes("Neko looks for trouble that repeats and suggests a fix. Off means it gathers nothing."), "the Suggestions row says what it does");
  check((await switchEl.getAttribute("aria-checked")) === "true", "it is on");
  await shot("improve-settings");
  await switchEl.click();
  await page.waitForFunction(() => document.querySelector("[data-setting='suggestions'] [role='switch']")?.getAttribute("aria-checked") === "false", null, { timeout: 8000 });
  check((await api({ Improve: "List" })).Improve.enabled === false, "switching it off tells the daemon");
  check(sent.some((b) => b.includes('"SetEnabled"') && b.includes('"enabled":false')), "with SetEnabled { enabled: false }");

  // ---- Off: a quiet notice with a way back ---------------------------------------------------------------------------
  await page.goto(`${base}/#/improvements`);
  const off = page.locator("[data-slot='improve-off']");
  await off.waitFor({ timeout: 15000 });
  check((await off.innerText()).includes("Suggestions are off. Neko is not looking for patterns."), "off reads as one quiet line");
  await shot("improve-off");
  await off.getByRole("button", { name: /Turn on/ }).click();
  await off.waitFor({ state: "detached", timeout: 8000 });
  check((await api({ Improve: "List" })).Improve.enabled === true, "its button turns suggestions back on");
  check(sent.some((b) => b.includes('"SetEnabled"') && b.includes('"enabled":true')), "with SetEnabled { enabled: true }");

  // ---- The daemon's answer replaced: empty, waiting in Skills, Apply on a pack ---------------------------------------------
  const answer = (over) => ({ Improve: { enabled: true, dev_checkout: false, proposals: [], open_pack: null, note: null, ...over } });
  const mock = async (handler) => {
    await page.route("**/api/request", async (route) => {
      const body = route.request().postData() ?? "";
      const out = body.includes('"Improve"') ? await handler(body) : null;
      if (out) await route.fulfill({ json: out });
      else await route.continue();
    });
  };

  await mock(() => answer({}));
  await page.goto(`${base}/#/improvements`);
  // The page is kept alive between visits and re-asks only slowly; a reload asks at once, through the mock.
  await page.reload();
  await page.locator("[data-slot='empty']").waitFor({ timeout: 15000 });
  check((await page.locator("[data-slot='empty']").innerText()).includes("Neko hasn't suggested anything yet."), "no proposals reads \"Neko hasn't suggested anything yet.\"");
  check((await page.locator("[data-slot='improvement']").count()) === 0 && (await page.locator("[data-slot='improve-off']").count()) === 0, "with no rows and no off notice");
  await shot("improve-empty");
  await a11y("improvements (empty)");
  await page.unroute("**/api/request");

  const real = (await api({ Improve: "List" })).Improve;
  const pack = { ...real.proposals.find((p) => p.id === "imp-helped"), id: "imp-mock-pack", state: "open", title: "Install the Spaceo pack", target: "spaceo", can_undo: false, undo_confirms: false, applied_ms: null };
  const waiting = { ...real.proposals.find((p) => p.id === "imp-bad"), id: "imp-mock-sent", state: "sent", kind: "skill_patch", title: "Tidy the review skill", can_undo: false, undo_confirms: false };
  await mock((body) => (body.includes('"Apply"') ? answer({ proposals: [{ ...pack, state: "sent" }], open_pack: "catalog:spaceo", note: "Opened the pack preview." }) : answer({ proposals: [pack, waiting] })));
  await page.goto(`${base}/#/improvements`);
  await page.reload();
  await row("imp-mock-pack").waitFor({ timeout: 15000 });
  const wr = row("imp-mock-sent");
  check((await wr.locator("[data-slot='status-glyph'][data-status='inProgress']").count()) === 1 && /Waiting in Skills/.test(await wr.innerText()), "a suggestion sent to Skills is amber with the words Waiting in Skills");
  check((await wr.getByRole("button", { name: /^(Apply|Reject|Undo) / }).count()) === 0, "and has no Apply, Reject or Undo");
  await row("imp-mock-pack").getByRole("button", { name: /^Apply / }).click();
  const preview = page.locator("[data-slot='pack-preview']");
  await preview.waitFor({ timeout: 15000 });
  check(/#\/packs/.test(await page.evaluate(() => location.hash)), "Apply on a pack suggestion goes to the Packs page");
  check(!!(await page.locator("[data-slot='add-pack']").count()) && (await preview.innerText()).length > 40, "and opens that pack's preview");
  check((await preview.getByRole("button", { name: "Install", exact: true }).count()) === 1 && sentWith('"Install"') === 0, "where Install is still the person's own step");
  await shot("improve-pack-preview");
  await page.unroute("**/api/request");
  await page.keyboard.press("Escape");
  await page.locator("[data-slot='add-pack']").waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
}
