import { useRef, useState, type ComponentPropsWithoutRef } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Copy } from "./icons";

const remarkPlugins = [remarkGfm];

/**
 * A table has no card: hairline rows under a muted header, scrolling sideways
 * when it is wider than the column rather than wrapping into narrow strips, and
 * a copy button below it that copies the cells as tab-separated text.
 */
function Table(props: ComponentPropsWithoutRef<"table">) {
  const ref = useRef<HTMLTableElement>(null);
  const [copied, setCopied] = useState(false);
  const copy = () => {
    const rows = [...(ref.current?.rows ?? [])].map((row) =>
      [...row.cells].map((cell) => (cell.textContent ?? "").replace(/\s+/g, " ").trim()).join("\t"),
    );
    navigator.clipboard?.writeText(rows.join("\n")).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };
  return (
    <div className="table-block">
      <div className="table-wrap" tabIndex={0}>
        <table ref={ref} {...props} />
      </div>
      <div className="table-foot">
        <button onClick={copy} title="Copy table" aria-label="Copy table">
          {copied ? <span>Copied</span> : <Copy />}
        </button>
      </div>
    </div>
  );
}

/** Prose with GFM: tables, task lists, strikethrough, autolinks. */
export function Markdown({ text }: { text: string }) {
  return (
    <div className="prose">
      <ReactMarkdown
        remarkPlugins={remarkPlugins}
        components={{
          a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer noopener" />,
          table: ({ node: _node, ...props }) => <Table {...props} />,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
