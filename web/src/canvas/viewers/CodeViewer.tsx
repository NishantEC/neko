// Code, JSON and plain text: highlighted by the same Shiki the replies use,
// read only. JSON is pretty-printed when it parses.
import { Suspense } from "react";
import { MarkdownCodeBlock, ShikiCodeBlock } from "@/chat/t3/MarkdownCodeBlock";
import { useViewerText, ViewerProblem } from "@/canvas/builtin-viewers";
import type { ViewerProps } from "@/canvas/viewers";

/** Shiki's name for a kind, where the extension is not already one. */
const LANGUAGE: Record<string, string> = { txt: "text", log: "text", mjs: "javascript", js: "javascript", ts: "typescript", py: "python", rb: "ruby", rs: "rust", kt: "kotlin", h: "c", sh: "shellscript", yaml: "yaml", patch: "diff" };

/** Past this, highlighting costs more than it gives: the text is shown plain. */
const MAX_HIGHLIGHT = 200_000;

export default function CodeViewer(props: ViewerProps) {
  const { text, error } = useViewerText(props);
  if (error) return <ViewerProblem message={error} />;
  if (text === null) return <p className="p-4 text-sm text-muted-foreground" aria-busy="true">Opening…</p>;
  let code = text;
  if (props.kind === "json") {
    try { code = JSON.stringify(JSON.parse(text), null, 2); } catch { /* shown as saved */ }
  }
  const language = LANGUAGE[props.kind] ?? props.kind;
  const plain = <pre className="whitespace-pre-wrap p-3 font-mono text-xs">{code}</pre>;
  return (
    <div data-slot="code-viewer" className="chat-markdown h-full overflow-auto px-4 py-3">
      <MarkdownCodeBlock code={code} language={language}>
        {code.length > MAX_HIGHLIGHT ? plain : (
          <Suspense fallback={plain}>
            <ShikiCodeBlock code={code} language={language} isStreaming={false} fallback={plain} />
          </Suspense>
        )}
      </MarkdownCodeBlock>
    </div>
  );
}
