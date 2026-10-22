// from hushbacks packages/ui/src/components/ai-elements/message-markdown.tsx
// (adapted: no KaTeX/math plugins — Neko replies carry tables, code and lists, not formulas)
import remarkGfm from "remark-gfm";
import { Streamdown, type StreamdownProps } from "streamdown";

const REMARK_PLUGINS = [remarkGfm];

export default function MessageMarkdown({ remarkPlugins, children, ...props }: StreamdownProps) {
  return (
    <Streamdown remarkPlugins={remarkPlugins ?? REMARK_PLUGINS} {...props}>
      {children}
    </Streamdown>
  );
}
