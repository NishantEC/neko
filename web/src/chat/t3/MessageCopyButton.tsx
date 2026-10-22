// from t3code apps/web/src/components/chat/MessageCopyButton.tsx @3b95e7831 (MIT)
// (adapted: a plain icon swap instead of MorphIcon, no anchored toast)
import { memo, useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";

export const MessageCopyButton = memo(function MessageCopyButton({
  text,
  label = "Copy message",
  variant = "outline",
  className,
}: {
  text: string;
  label?: string;
  variant?: "outline" | "ghost";
  className?: string;
}) {
  const [isCopied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), []);
  const copy = () => {
    void navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true);
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            aria-label={label}
            disabled={isCopied}
            onClick={copy}
            type="button"
            size="xs"
            variant={variant}
            className={cn("h-6 px-1.5", variant === "ghost" && "text-muted-foreground hover:text-foreground", className)}
          />
        }
      >
        {isCopied ? <Check className="size-3 text-primary" /> : <Copy className="size-3" />}
      </TooltipTrigger>
      <TooltipContent>
        <p>{isCopied ? "Copied" : label}</p>
      </TooltipContent>
    </Tooltip>
  );
});
