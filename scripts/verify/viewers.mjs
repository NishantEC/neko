// Everything renders (docs/superpowers/specs/2026-11-08-everything-renders-design.md,
// Phase A): artifacts of any kind open in the viewer registered for their
// kind, a reply's ```csv block is drawn as a table, the sketch saves its next
// version, and an agent's ShowArtifact / OpenTab reach the open page.
//
//   NEKO_WEB_ONLY=viewers node scripts/web-verify.mjs
import { writeFileSync } from "node:fs";
import path from "node:path";

/** A tetrahedron as a binary glTF (positions only, no material), built here so no fixture is needed. */
function tetrahedronGlb() {
  const a = [0, 1, 0], b = [-1, -0.6, 0.8], c = [1, -0.6, 0.8], d = [0, -0.6, -1];
  const faces = [[a, b, c], [a, c, d], [a, d, b], [b, d, c]];
  const positions = new Float32Array(faces.flat(2));
  const bin = Buffer.from(positions.buffer);
  const json = {
    asset: { version: "2.0" }, scene: 0, scenes: [{ nodes: [0] }], nodes: [{ mesh: 0 }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 } }] }],
    buffers: [{ byteLength: bin.length }], bufferViews: [{ buffer: 0, byteOffset: 0, byteLength: bin.length }],
    accessors: [{ bufferView: 0, componentType: 5126, count: positions.length / 3, type: "VEC3", min: [-1, -0.6, -1], max: [1, 1, 0.8] }],
  };
  let text = Buffer.from(JSON.stringify(json));
  text = Buffer.concat([text, Buffer.alloc((4 - (text.length % 4)) % 4, 0x20)]);
  const header = Buffer.alloc(12);
  header.write("glTF", 0, "ascii");
  header.writeUInt32LE(2, 4);
  header.writeUInt32LE(12 + 8 + text.length + 8 + bin.length, 8);
  const chunk = (body, type) => {
    const head = Buffer.alloc(8);
    head.writeUInt32LE(body.length, 0);
    head.write(type, 4, "ascii");
    return Buffer.concat([head, body]);
  };
  return Buffer.concat([header, chunk(text, "JSON"), chunk(bin, "BIN\0")]);
}

const SKETCH = {
  type: "excalidraw", version: 2, source: "neko-verify",
  elements: [
    { type: "rectangle", id: "box-1", x: 80, y: 80, width: 220, height: 110, angle: 0, strokeColor: "#1e1e1e", backgroundColor: "#a5d8ff", fillStyle: "solid", strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [], frameId: null, roundness: { type: 3 }, seed: 11, version: 1, versionNonce: 1, isDeleted: false, boundElements: null, updated: 1, link: null, locked: false },
    { type: "rectangle", id: "box-2", x: 420, y: 80, width: 220, height: 110, angle: 0, strokeColor: "#1e1e1e", backgroundColor: "#b2f2bb", fillStyle: "solid", strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [], frameId: null, roundness: { type: 3 }, seed: 12, version: 1, versionNonce: 2, isDeleted: false, boundElements: null, updated: 1, link: null, locked: false },
    { type: "text", id: "label-1", x: 110, y: 120, width: 160, height: 35, angle: 0, strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid", strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [], frameId: null, roundness: null, seed: 13, version: 1, versionNonce: 3, isDeleted: false, boundElements: null, updated: 1, link: null, locked: false, text: "Sign in", originalText: "Sign in", fontSize: 28, fontFamily: 5, textAlign: "left", verticalAlign: "top", containerId: null, autoResize: true, lineHeight: 1.25 },
  ],
  appState: { viewBackgroundColor: "#ffffff" },
  files: {},
};

const DOC = `# Retry plan

Requests that time out are **tried twice more** with a short back-off.

- 200 ms, then 800 ms
- only idempotent calls

\`\`\`csv
attempt,wait
1,none
2,200 ms
\`\`\`
`;

export default async function ({ page, shot, a11y, check, sleep, PORT, DATA }) {
  const home = `http://127.0.0.1:${PORT}/#/`;
  // Excalidraw's fonts come from Neko itself, never another site.
  const fonts = { local: 0 };
  const onFont = (response) => { if (response.url().includes("/excalidraw/fonts/") && response.ok()) fonts.local++; };
  page.on("response", onFont);
  await page.goto(home);
  await page.waitForSelector("[data-slot='neko-turn']", { timeout: 20000 });
  const request = (body) => page.evaluate(async (body) => (await fetch("/api/request", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json(), body);

  // A reply's ```csv block is a table, not code.
  const fence = page.locator("[data-slot='reply-fence'][data-viewer='csv']").first();
  await fence.locator("table").waitFor({ timeout: 15000 }).then(() => check(true, "a reply's ```csv block is drawn as a table"), () => check(false, "a reply's ```csv block is drawn as a table"));
  const headers = await fence.locator("th").allInnerTexts().catch(() => []);
  check(headers.join("|") === "endpoint|timeouts|retried, then ok", `the fence's header row is the first line, quotes kept (${headers.join("|")})`);
  check((await fence.locator("tbody tr").count()) === 2, "the fence's two data rows are drawn");
  await fence.scrollIntoViewIfNeeded();
  await shot("viewers-fence-csv");

  // Each kind opens in its own viewer on the canvas beside the chat.
  const kinds = [
    { name: "model.glb", body: tetrahedronGlb(), viewer: "3d", ready: "[data-slot='model-viewer'][data-loaded='true'] canvas", title: "Tetrahedron" },
    { name: "timeouts.csv", body: "endpoint,timeouts\n/orders,14\n/users,3\n", viewer: "csv", ready: "[data-slot='csv-viewer'] table th", title: "Timeouts" },
    { name: "plan.md", body: DOC, viewer: "markdown", ready: "[data-slot='markdown-viewer'] h1", title: "Retry plan" },
    { name: "flow.excalidraw", body: JSON.stringify(SKETCH), viewer: "excalidraw", ready: "[data-slot='sketch-viewer'] canvas", title: "Login flow sketch" },
  ];
  const saved = {};
  for (const kind of kinds) {
    const file = path.join(DATA, `viewers-${kind.name}`);
    writeFileSync(file, kind.body);
    const reply = await request({ SaveArtifact: { title: kind.title, file, workspace_id: null, artifact_id: null, note: "First pass" } });
    const artifact = reply?.ArtifactSaved?.artifact;
    check(!!artifact && artifact.kind === kind.name.split(".").pop(), `SaveArtifact keeps a .${kind.name.split(".").pop()} (kind ${artifact?.kind})`);
    if (!artifact) continue;
    saved[kind.viewer] = artifact;
    const panel = `[data-slot='artifact-panel'][data-artifact='${artifact.id}']`;
    await page.waitForSelector(`${panel} [data-slot='artifact-view'][data-viewer='${kind.viewer}']`, { timeout: 10000 })
      .then(() => check(true, `a .${artifact.kind} opens beside the chat in the ${kind.viewer} viewer (shown without being asked: a reply is running)`), () => check(false, `a .${artifact.kind} opens in the ${kind.viewer} viewer`));
    await page.waitForSelector(`${panel} ${kind.ready}`, { timeout: 20000 })
      .then(() => check(true, `the ${kind.viewer} viewer draws it`), () => check(false, `the ${kind.viewer} viewer draws it`));
    await sleep(kind.viewer === "excalidraw" || kind.viewer === "3d" ? 1200 : 300);
    await shot(`viewers-${kind.viewer}`);
  }
  const served = await page.evaluate(async (id) => (await fetch(`/artifact/${id}/1`)).headers.get("content-type"), saved["3d"]?.id);
  check(served === "model/gltf-binary", `a .glb is served as model/gltf-binary (${served})`);
  if (saved.markdown) {
    await request({ ShowArtifact: { id: saved.markdown.id, version: null } });
    const doc = page.locator(`[data-slot='artifact-panel'][data-artifact='${saved.markdown.id}'] [data-slot='markdown-viewer']`);
    await doc.locator("h1").waitFor({ timeout: 10000 }).catch(() => {});
    check((await doc.locator("h1").innerText().catch(() => "")) === "Retry plan" && (await doc.locator("strong").count()) === 1, "the document is rendered Markdown, not shown as text");
    check((await doc.locator("[data-slot='reply-fence'][data-viewer='csv'] table").count()) === 1, "a ```csv block inside the document is a table too");
  }

  // The 3D toolbar: wireframe and fit act on the model.
  if (saved["3d"]) {
    const reply = await request({ ShowArtifact: { id: saved["3d"].id, version: null } });
    check(reply?.Shown?.pages >= 1, `ShowArtifact reaches the open page (${reply?.Shown?.pages} page)`);
    await page.waitForSelector(`[data-slot='artifact-panel'][data-artifact='${saved["3d"].id}'] [data-slot='model-viewer'][data-loaded='true']`, { timeout: 15000 })
      .then(() => check(true, "ShowArtifact brings the model back beside the chat"), () => check(false, "ShowArtifact brings the model back beside the chat"));
    const triangles = await page.locator("[data-slot='model-viewer'] [role='toolbar']").innerText();
    check(/4 triangles/.test(triangles), `the model's four faces are counted (${triangles.replace(/\s+/g, " ").trim()})`);
    await page.locator("[data-slot='model-viewer'] [role='switch'][aria-label='Wireframe']").click();
    await sleep(400);
    await shot("viewers-3d-wireframe");
    await page.locator("[data-slot='model-viewer'] [role='switch'][aria-label='Wireframe']").click();
  }

  // The sketch: draw a rectangle, save, and the panel moves to v2.
  if (saved.excalidraw) {
    await request({ ShowArtifact: { id: saved.excalidraw.id, version: null } });
    const board = page.locator(`[data-slot='artifact-panel'][data-artifact='${saved.excalidraw.id}'] [data-slot='sketch-board']`);
    await board.locator("canvas").first().waitFor({ timeout: 15000 });
    await sleep(800);
    const box = await board.boundingBox();
    if (box) {
      await page.mouse.click(box.x + box.width - 40, box.y + box.height - 40);
      await page.keyboard.press("r");
      await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.6);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.8, { steps: 8 });
      await page.mouse.up();
    }
    const saveButton = page.getByRole("button", { name: /Save as v2/ });
    await saveButton.waitFor({ timeout: 5000 });
    await page.waitForFunction(() => {
      const b = [...document.querySelectorAll("button")].find((x) => /Save as v2/.test(x.textContent ?? ""));
      return b && !b.disabled;
    }, null, { timeout: 5000 }).then(() => check(true, "drawing on the sketch makes it unsaved"), () => check(false, "drawing on the sketch makes it unsaved"));
    await shot("viewers-excalidraw-edited");
    await saveButton.click();
    await page.waitForSelector(`[data-slot='artifact-panel'][data-artifact='${saved.excalidraw.id}'][data-version='2']`, { timeout: 10000 })
      .then(() => check(true, "saving the sketch keeps it as v2 and the panel shows it"), () => check(false, "saving the sketch keeps it as v2"));
    const record = await request({ Artifact: { id: saved.excalidraw.id } });
    const elements = await page.evaluate(async (id) => (await (await fetch(`/artifact/${id}/2`)).json()).elements.length, saved.excalidraw.id);
    check(record?.Artifact?.artifact?.versions?.length === 2 && elements === 4, `v2 holds the new rectangle (${elements} elements)`);
  }

  // From another tab, ShowArtifact opens the artifact's own tab; OpenTab opens a route.
  await page.locator("button[aria-label='Close the inspector']").click().catch(() => {});
  await page.goto(`http://127.0.0.1:${PORT}/#/tasks`);
  await page.waitForSelector("[data-slot='page'][data-route='tasks']", { timeout: 10000 });
  if (saved.csv) {
    await request({ ShowArtifact: { id: saved.csv.id, version: 1 } });
    await page.waitForSelector(`[data-slot='artifact-tab'] [data-slot='artifact-panel'][data-artifact='${saved.csv.id}'] [data-slot='csv-viewer'] table`, { timeout: 10000 })
      .then(() => check(true, "ShowArtifact away from Home opens the artifact in its own tab"), () => check(false, "ShowArtifact away from Home opens the artifact in its own tab"));
    await shot("viewers-tab-csv");
    await a11y("artifact tab (csv)");
  }
  const tab = await request({ OpenTab: { route: "calendar" } });
  check(tab?.Shown?.pages >= 1, "OpenTab is accepted");
  await page.waitForFunction(() => location.hash.startsWith("#/calendar"), null, { timeout: 10000 })
    .then(() => check(true, "OpenTab opens the route in a tab"), () => check(false, "OpenTab opens the route in a tab"));
  const refused = await request({ OpenTab: { route: "https://example.com" } });
  check(!!refused?.Error, "OpenTab refuses another site");
  check(fonts.local > 0, `the sketch's fonts load from Neko itself (${fonts.local} font files)`);
  page.off("response", onFont);
  await page.goto(home);
}
