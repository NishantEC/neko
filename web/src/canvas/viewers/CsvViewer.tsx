// A table: .csv / .tsv artifacts and a reply's ```csv / ```tsv block. The
// first row is the header. Parsed here (RFC 4180 quoting, CRLF or LF), drawn
// in the reply table's own frame (copy as Markdown or CSV, wrap toggle).
import { MarkdownTable } from "@/chat/t3/MarkdownTable";
import { useViewerText, ViewerProblem } from "@/canvas/builtin-viewers";
import type { ViewerProps } from "@/canvas/viewers";

/** Rows shown at most; the rest are counted, never drawn. */
const MAX_ROWS = 2000;

/** Splits delimited text into rows of cells; a quoted cell may hold the delimiter, quotes ("") and newlines. */
export function parseDelimited(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') { cell += '"'; i++; }
      else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"' && cell === "") quoted = true;
    else if (c === delimiter) { row.push(cell); cell = ""; }
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
    } else cell += c;
  }
  if (cell !== "" || row.length) { row.push(cell); rows.push(row); }
  return rows.filter((r) => r.length > 1 || r[0] !== "");
}

export default function CsvViewer(props: ViewerProps) {
  const { text, error } = useViewerText(props);
  if (error) return <ViewerProblem message={error} />;
  if (text === null) return <p className="p-4 text-sm text-muted-foreground" aria-busy="true">Opening…</p>;
  const delimiter = props.kind === "tsv" ? "\t" : ",";
  const [header = [], ...body] = parseDelimited(text.replace(/^﻿/, ""), delimiter);
  const width = Math.max(header.length, ...body.slice(0, MAX_ROWS).map((r) => r.length));
  const shown = body.slice(0, MAX_ROWS);
  const pad = (r: string[]) => [...r, ...Array(Math.max(0, width - r.length)).fill("")];
  if (!header.length) return <p className="p-4 text-sm text-muted-foreground">This table is empty.</p>;
  return (
    <div data-slot="csv-viewer" className={props.inline ? "chat-markdown" : "chat-markdown h-full overflow-auto px-4 py-3"}>
      <MarkdownTable>
        <thead>
          <tr>{pad(header).map((c, i) => <th key={i}>{c}</th>)}</tr>
        </thead>
        <tbody>
          {shown.map((r, i) => <tr key={i}>{pad(r).map((c, j) => <td key={j}>{c}</td>)}</tr>)}
        </tbody>
      </MarkdownTable>
      <p className="mt-1 text-xs text-muted-foreground tabular-nums">
        {body.length} row{body.length === 1 ? "" : "s"}{body.length > MAX_ROWS ? `, the first ${MAX_ROWS} shown` : ""}
      </p>
    </div>
  );
}
