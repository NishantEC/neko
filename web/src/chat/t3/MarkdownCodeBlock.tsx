// from t3code apps/web/src/components/chat/MarkdownCodeBlock.tsx @3b95e7831 (MIT)
// (adapted: no Pierre file icons, run-in-terminal or mermaid; Shiki's web bundle,
// loaded lazily, with a light/dark dual theme instead of T3's diff theme setting)
import { Check, Copy, WrapText } from "lucide-react";
import { use, useEffect, useRef, useState, type ReactNode } from "react";

import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";

export function extractFenceLanguage(className: string | undefined): string {
  const match = className?.match(/(?:^|\s)language-([^\s]+)/);
  return match?.[1]?.toLowerCase() ?? "text";
}

const ghostMuted = "text-muted-foreground hover:text-foreground size-6 rounded-md";

export function MarkdownCodeBlockFrame({
  language,
  wrapped = true,
  title,
  actions,
  children,
}: {
  language: string;
  wrapped?: boolean;
  title?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      dir="ltr"
      className="chat-markdown-codeblock my-[0.65rem] overflow-hidden rounded-lg border border-border/70 bg-secondary leading-snug dark:border-transparent dark:bg-input/32"
      data-language={language}
      data-wrap={wrapped ? "true" : "false"}
    >
      <div className="chat-markdown-codeblock-header flex items-center justify-between gap-2 pt-1.5 pr-1.5 pb-0 pl-3 select-none">
        {title ?? (
          <span className="inline-flex min-w-0 items-center gap-1.5 font-mono text-2xs">
            <span className="truncate">{language}</span>
          </span>
        )}
        {actions}
      </div>
      {children}
    </div>
  );
}

export function MarkdownCodeBlock({
  code,
  language,
  children,
}: {
  code: string;
  language: string;
  children: ReactNode;
}) {
  const [copied, setCopied] = useState(false);
  const [wrapped, setWrapped] = useState(true);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wrapLabel = wrapped ? "Disable line wrap" : "Wrap lines";
  const copyLabel = copied ? "Copied" : "Copy code";

  const handleCopy = () => {
    if (typeof navigator === "undefined" || navigator.clipboard == null) return;
    void navigator.clipboard
      .writeText(code)
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
    <MarkdownCodeBlockFrame
      language={language}
      wrapped={wrapped}
      actions={
        <span className="flex items-center gap-0.5" role="toolbar" aria-label="Code block actions">
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant={wrapped ? "secondary" : "ghost"}
                  size="icon-xs"
                  className={ghostMuted}
                  aria-pressed={wrapped}
                  onClick={() => setWrapped((value) => !value)}
                  aria-label={wrapLabel}
                />
              }
            >
              <WrapText className="size-3" />
            </TooltipTrigger>
            <TooltipContent side="top">{wrapLabel}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger
              render={<Button type="button" variant="ghost" size="icon-xs" className={ghostMuted} onClick={handleCopy} aria-label={copyLabel} />}
            >
              {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
            </TooltipTrigger>
            <TooltipContent side="top">{copyLabel}</TooltipContent>
          </Tooltip>
        </span>
      }
    >
      {children}
    </MarkdownCodeBlockFrame>
  );
}

// ---- highlighting -------------------------------------------------------------

const cache = new Map<string, string>();
const MAX_CACHE = 300;
let highlighter: Promise<typeof import("shiki/bundle/web")> | null = null;
const loadShiki = () => (highlighter ??= import("shiki/bundle/web"));
const pending = new Map<string, Promise<string>>();

function highlight(code: string, language: string): Promise<string> {
  const key = `${language}\u0000${code}`;
  const hit = pending.get(key);
  if (hit) return hit;
  const job = loadShiki()
    .then(async (shiki) => {
      const lang = language in shiki.bundledLanguages ? language : "text";
      return shiki.codeToHtml(code, { lang, themes: { light: "github-light", dark: "github-dark" }, defaultColor: "light" });
    })
    // A grammar that fails to load (or Shiki itself) leaves the code as plain text, never an error.
    .catch(() => `<pre><code>${code.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c] as string)}</code></pre>`)
    .then((html) => {
      if (cache.size >= MAX_CACHE) cache.delete(cache.keys().next().value as string);
      cache.set(key, html);
      pending.delete(key);
      return html;
    });
  pending.set(key, job);
  return job;
}

function Highlighted({ code, language }: { code: string; language: string }) {
  const html = use(highlight(code, language));
  return <div className="chat-markdown-shiki" dangerouslySetInnerHTML={{ __html: html }} />;
}

/** Shiki-highlighted code; plain text while a reply streams or Shiki loads. */
export function ShikiCodeBlock({ code, language, isStreaming, fallback }: { code: string; language: string; isStreaming: boolean; fallback: ReactNode }) {
  const cached = cache.get(`${language}\u0000${code}`);
  if (cached) return <div className="chat-markdown-shiki" dangerouslySetInnerHTML={{ __html: cached }} />;
  if (isStreaming) return <>{fallback}</>;
  return <Highlighted code={code} language={language} />;
}
