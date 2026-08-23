// Proves the protocol client before any UI exists: if this cannot talk to the
// daemon, nothing rendered on top of it would work either.
import { NekoClient } from "./protocol.mjs";

const client = new NekoClient();
await client.connect();
console.log("connected to neko-daemon");

for (const query of ["", "finder"]) {
  let partials = 0;
  const response = await client.search(query, { onPartial: () => partials++ });
  const items = response.SearchResults.items;
  console.log(`\nquery ${JSON.stringify(query)} — ${items.length} rows, ${partials} partial frame(s)`);
  for (const item of items.slice(0, 4)) {
    console.log(`   [${item.section_label}] ${item.title.slice(0, 40)}`);
  }
}
process.exit(0);
