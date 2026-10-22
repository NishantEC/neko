// from t3code apps/web/src/components/ChatMarkdown.tsx @3b95e7831 (MIT)
// (adapted: the root, the react-markdown pipeline and the pre / code / table /
// link renderers. Dropped: file-link chips, thread / PR / citation links, math,
// mermaid, images from the workspace, skills, find-in-thread, the incremental
// streaming parser — none of which Neko's daemon produces.)
import { memo, Suspense, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "./remark-breaks";

import { cn } from "@/ui/lib/utils";
import { REPLY_KINDS, ReplyBlock } from "@/chat/reply-views";
import { MarkdownCodeBlock, ShikiCodeBlock, extractFenceLanguage } from "./MarkdownCodeBlock";
import { MarkdownTable } from "./MarkdownTable";

export interface ChatMarkdownProps {
  text: string;
  className?: string;
  inline?: boolean;
  lineBreaks?: boolean;
  isStreaming?: boolean;
}

function nodeToPlainText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(nodeToPlainText).join("");
  if (typeof node === "object" && "props" in node) return nodeToPlainText((node as { props: { children?: ReactNode } }).props.children);
  return "";
}

function extractCodeBlock(children: ReactNode): { className: string | undefined; code: string } | null {
  const child = Array.isArray(children) ? children[0] : children;
  if (!child || typeof child !== "object" || !("props" in child)) return null;
  const props = (child as { props: { className?: string; children?: ReactNode } }).props;
  return { className: props.className, code: nodeToPlainText(props.children) };
}

function components(isStreaming: boolean): Components {
  return {
    a: function MarkdownLink({ node: _node, href, ...props }) {
      const external = typeof href === "string" && /^https?:/i.test(href);
      return <a {...props} href={href} {...(external ? { target: "_blank", rel: "noreferrer" } : {})} />;
    },
    table: function MarkdownTableRenderer({ node: _node, ...props }) {
      return <MarkdownTable {...props} />;
    },
    pre: function MarkdownPre({ node: _node, children, ...props }) {
      const codeBlock = extractCodeBlock(children);
      if (!codeBlock) return <pre {...props}>{children}</pre>;
      const language = extractFenceLanguage(codeBlock.className);
      if (REPLY_KINDS.has(language) && !isStreaming) {
        const block = ReplyBlock({ kind: language, source: codeBlock.code });
        if (block) return <>{block}</>;
      }
      const plain = <pre {...props}>{children}</pre>;
      return (
        <MarkdownCodeBlock code={codeBlock.code} language={language}>
          <Suspense fallback={plain}>
            <ShikiCodeBlock code={codeBlock.code.replace(/\n$/, "")} language={language} isStreaming={isStreaming} fallback={plain} />
          </Suspense>
        </MarkdownCodeBlock>
      );
    },
  };
}

const STREAMING = components(true);
const SETTLED = components(false);
const PLUGINS = [remarkGfm];
const PLUGINS_WITH_BREAKS = [remarkGfm, remarkBreaks];

function ChatMarkdown({ text, className, inline = false, lineBreaks = false, isStreaming = false }: ChatMarkdownProps) {
  const Root = inline ? "span" : "div";
  return (
    <Root
      className={cn(
        "chat-markdown w-full min-w-0 [overflow-wrap:anywhere] [word-break:break-word]",
        !inline && "text-sm leading-relaxed text-foreground",
        className,
      )}
      data-streaming={isStreaming ? "" : undefined}
    >
      <ReactMarkdown
        remarkPlugins={lineBreaks ? PLUGINS_WITH_BREAKS : PLUGINS}
        components={isStreaming ? STREAMING : SETTLED}
        {...(inline ? { allowedElements: ["p", "strong", "em", "code", "a", "del"], unwrapDisallowed: true } : {})}
      >
        {text}
      </ReactMarkdown>
    </Root>
  );
}

export default memo(ChatMarkdown);
