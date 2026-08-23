// neko's panel, in React on GPUIX — the same daemon, a different client.
//
// Deliberately a like-for-like of the Rust panel's structure (input row,
// grouped results, footer) so the comparison is about the *client*, not about
// two different designs. Colours are neko's own `neutral` palette values,
// copied here rather than derived: this is a spike, and a real port would read
// them from the daemon or share a token file.
//
// No JSX, so this runs on plain `node` with no build step. That is a property
// of the spike, not a recommendation — a real client would use the JSX
// runtime `@gpuix/react` ships.

import React, { useCallback, useEffect, useRef, useState } from "react";
import { render } from "@gpuix/react";
import { NekoClient } from "./protocol.mjs";

const h = React.createElement;

// neko's `neutral` palette (crates/neko/src/theme.rs), at the values the Rust
// client renders today.
const T = {
  panel: "#131313",
  raised: "#1c1c1c",
  selected: "#2a2a2a",
  textPrimary: "#f2f2f2",
  textSecondary: "#a8a8a8",
  textTertiary: "#6e6e6e",
  hairline: "#2c2c2c",
};

const ROW_HEIGHT = 40;
const PANEL_WIDTH = 760;

function useNeko() {
  const client = useRef(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    const c = new NekoClient();
    c.connect().then(
      () => {
        client.current = c;
        setReady(true);
      },
      (e) => setError(String(e)),
    );
  }, []);

  return { client, ready, error };
}

function Row({ item, selected }) {
  return h(
    "div",
    {
      style: {
        display: "flex",
        alignItems: "center",
        gap: 12,
        height: ROW_HEIGHT,
        paddingLeft: 20,
        paddingRight: 20,
        borderRadius: 8,
        backgroundColor: selected ? T.selected : "transparent",
      },
    },
    h("div", {
      style: {
        width: 22,
        height: 22,
        borderRadius: 6,
        backgroundColor: T.raised,
        flexShrink: 0,
      },
    }),
    h("div", { style: { color: T.textPrimary, fontSize: 14 } }, item.title),
    item.subtitle
      ? h("div", { style: { color: T.textTertiary, fontSize: 12 } }, item.subtitle)
      : null,
  );
}

function Panel() {
  const { client, ready, error } = useNeko();
  const [query, setQuery] = useState("");
  const [items, setItems] = useState([]);
  const [selected, setSelected] = useState(0);
  // Every response carries the query it answered, so a slow one landing after
  // a faster one cannot overwrite it — the same "a superseded query is
  // abandoned" rule the Rust client enforces daemon-side.
  const generation = useRef(0);

  const runSearch = useCallback(
    async (text) => {
      if (!client.current) return;
      const mine = ++generation.current;
      const apply = (rows) => {
        if (generation.current !== mine) return;
        setItems(rows);
        setSelected(0);
      };
      const response = await client.current.search(text, { limit: 8, onPartial: apply });
      apply(response.SearchResults.items);
    },
    [client],
  );

  useEffect(() => {
    if (ready) runSearch("");
  }, [ready, runSearch]);

  if (error) {
    return h(
      "div",
      { style: { padding: 24, backgroundColor: T.panel, color: T.textPrimary } },
      `Can't reach neko-daemon: ${error}`,
    );
  }

  // Contiguous runs of one provider share a header, exactly as
  // `panel::group_into_sections` does.
  const sections = [];
  for (const item of items) {
    const last = sections[sections.length - 1];
    if (last && last.kind === item.kind) last.items.push(item);
    else sections.push({ kind: item.kind, label: item.section_label, items: [item] });
  }

  const activateSelected = () => {
    const item = items[selected];
    if (!item || !client.current) return;
    client.current.activate(item.kind, item.id, { query });
  };

  let index = -1;
  return h(
    "div",
    {
      style: {
        display: "flex",
        flexDirection: "column",
        width: PANEL_WIDTH,
        height: 448,
        backgroundColor: T.panel,
        borderRadius: 16,
        overflow: "hidden",
      },
    },
    h(
      "div",
      {
        style: {
          display: "flex",
          alignItems: "center",
          gap: 12,
          height: 56,
          paddingLeft: 20,
          paddingRight: 20,
        },
      },
      h("div", {
        style: { width: 16, height: 16, borderRadius: 8, borderWidth: 2, borderColor: T.textTertiary },
      }),
      // GPUIX's own native text input — the Rust client has ~1,000 lines of
      // `text_field.rs` behind this (cursor, selection, word motion, the real
      // NSPasteboard) and here it is one element. This is the single clearest
      // example of what the React client buys.
      h("input", {
        autoFocus: true,
        value: query,
        placeholder: "Search apps and clipboard…",
        style: { flexGrow: 1, fontSize: 18, color: T.textPrimary },
        // **Enter is `onSubmit` on the input, not a key handler on a
        // container.** GPUIX's own `examples/chat.tsx` composer does exactly
        // this. A div's `onKeyDown` never fired here at all — the div never
        // takes focus, so there was nothing for it to receive.
        onSubmit: () => activateSelected(),
        // Arrows go to the focused element, which is the input.
        onKeyDown: (event) => {
          const key = event?.key ?? event?.keystroke ?? "";
          if (key === "down" || key === "ArrowDown") {
            setSelected((i) => Math.min(i + 1, Math.max(items.length - 1, 0)));
          } else if (key === "up" || key === "ArrowUp") {
            setSelected((i) => Math.max(i - 1, 0));
          }
        },
        onChange: (event) => {
          const text = typeof event === "string" ? event : (event?.value ?? "");
          setQuery(text);
          runSearch(text);
        },
      }),
    ),
    h(
      "div",
      { style: { display: "flex", flexDirection: "column", flexGrow: 1, paddingLeft: 8, paddingRight: 8 } },
      ...sections.flatMap((section) => [
        h(
          "div",
          {
            key: `h-${section.kind}`,
            style: { color: T.textTertiary, fontSize: 11, paddingLeft: 12, height: 28, display: "flex", alignItems: "center" },
          },
          section.label,
        ),
        ...section.items.map((item) => {
          index += 1;
          return h(Row, { key: `${item.kind}:${item.id}`, item, selected: index === selected });
        }),
      ]),
    ),
    h(
      "div",
      {
        style: {
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          height: 44,
          paddingLeft: 20,
          paddingRight: 20,
          borderTopWidth: 1,
          borderTopColor: T.hairline,
        },
      },
      h("div", { style: { color: T.textSecondary, fontSize: 12 } }, ready ? "neko · GPUIX client" : "connecting…"),
      h("div", { style: { color: T.textTertiary, fontSize: 12 } }, `${items.length} results`),
    ),
  );
}

render(h(Panel));
