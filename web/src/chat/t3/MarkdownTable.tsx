// from t3code apps/web/src/components/chat/MarkdownTable.tsx @3b95e7831 (MIT)
// (adapted: Neko's dropdown menu and a plain scrolling div instead of T3's
// ScrollArea; the Markdown / CSV serialisers are reimplemented here)
import { Check, Copy, Maximize2, Minimize2 } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ComponentProps } from "react";

import { Button } from "@/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";

const ghostMuted = "text-muted-foreground hover:text-foreground size-6 rounded-md";

function rowsOf(table: HTMLTableElement): string[][] {
  return Array.from(table.rows).map((row) => Array.from(row.cells).map((cell) => (cell.textContent ?? "").trim()));
}

function toMarkdown(table: HTMLTableElement): string {
  const rows = rowsOf(table);
  if (!rows.length) return "";
  const line = (cells: string[]) => `| ${cells.map((c) => c.replace(/\|/g, "\\|")).join(" | ")} |`;
  return [line(rows[0]), line(rows[0].map(() => "---")), ...rows.slice(1).map(line)].join("\n");
}

function toCsv(table: HTMLTableElement): string {
  const quote = (c: string) => (/[",\n]/.test(c) ? `"${c.replace(/"/g, '""')}"` : c);
  return rowsOf(table).map((r) => r.map(quote).join(",")).join("\n");
}

/** Whether any cell is clipped in the unwrapped layout; only then does the toggle change anything. */
function tableHasClippedCells(container: HTMLElement, table: HTMLTableElement): boolean {
  const previous = container.dataset.expanded;
  container.dataset.expanded = "false";
  try {
    for (const row of table.rows) for (const cell of row.cells) if (cell.scrollWidth > cell.clientWidth) return true;
    return false;
  } finally {
    if (previous === undefined) delete container.dataset.expanded;
    else container.dataset.expanded = previous;
  }
}

export function MarkdownTable({ children, ...props }: ComponentProps<"table">) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const tableRef = useRef<HTMLTableElement | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [hasClippedCells, setHasClippedCells] = useState(false);
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const expandLabel = expanded ? "Collapse table cells" : "Expand table cells";
  const copyLabel = copied ? "Copied" : "Copy table";

  const measureClippedCells = () => {
    const container = containerRef.current;
    const table = tableRef.current;
    if (!container || !table) return;
    const clipped = tableHasClippedCells(container, table);
    setHasClippedCells((was) => (was === clipped ? was : clipped));
  };

  useLayoutEffect(measureClippedCells);

  useLayoutEffect(() => {
    const table = tableRef.current;
    if (!table || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measureClippedCells);
    observer.observe(table);
    return () => observer.disconnect();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const handleCopy = (format: "markdown" | "csv") => {
    const table = tableRef.current;
    if (!table || navigator.clipboard == null) return;
    void navigator.clipboard
      .writeText(format === "markdown" ? toMarkdown(table) : toCsv(table))
      .then(() => {
        if (copiedTimerRef.current != null) clearTimeout(copiedTimerRef.current);
        setCopied(true);
        copiedTimerRef.current = setTimeout(() => {
          setCopied(false);
          copiedTimerRef.current = null;
        }, 1200);
      })
      .catch(() => {});
  };

  useEffect(
    () => () => {
      if (copiedTimerRef.current != null) clearTimeout(copiedTimerRef.current);
    },
    [],
  );

  return (
    <div ref={containerRef} className="chat-markdown-table-container" data-expanded={expanded ? "true" : "false"}>
      <div className="w-full max-w-full overflow-x-auto">
        <table ref={tableRef} {...props}>
          {children}
        </table>
      </div>
      <div className="mt-0.5 flex items-center justify-between select-none">
        {hasClippedCells ? (
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant={expanded ? "secondary" : "ghost"}
                  size="icon-xs"
                  className={ghostMuted}
                  aria-pressed={expanded}
                  onClick={() => setExpanded((value) => !value)}
                  aria-label={expandLabel}
                />
              }
            >
              {expanded ? <Minimize2 className="size-3" /> : <Maximize2 className="size-3" />}
            </TooltipTrigger>
            <TooltipContent side="top">{expandLabel}</TooltipContent>
          </Tooltip>
        ) : (
          <span />
        )}
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button type="button" variant="ghost" size="icon-xs" className={ghostMuted} aria-label={copyLabel} />}>
            {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => handleCopy("markdown")}>Copy as Markdown</DropdownMenuItem>
            <DropdownMenuItem onClick={() => handleCopy("csv")}>Copy as CSV</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
