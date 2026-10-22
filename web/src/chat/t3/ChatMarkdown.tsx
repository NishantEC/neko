// T3 Code's ChatMarkdown (ChatMarkdownRenderer.tsx), loaded lazily so the
// markdown pipeline (react-markdown, remark-gfm, micromark) stays out of the
// main chunk, as Streamdown did before it. Until it arrives the text shows as
// plain wrapped text in the same root, so nothing jumps.
import { lazy, memo, Suspense } from "react";
import { cn } from "@/ui/lib/utils";
import type { ChatMarkdownProps } from "./ChatMarkdownRenderer";

const Renderer = lazy(() => import("./ChatMarkdownRenderer"));

function ChatMarkdown(props: ChatMarkdownProps) {
  const Root = props.inline ? "span" : "div";
  return (
    <Suspense
      fallback={
        <Root className={cn("chat-markdown w-full min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]", !props.inline && "text-sm leading-relaxed text-foreground", props.className)}>
          {props.text}
        </Root>
      }
    >
      <Renderer {...props} />
    </Suspense>
  );
}

export type { ChatMarkdownProps };
export default memo(ChatMarkdown);
