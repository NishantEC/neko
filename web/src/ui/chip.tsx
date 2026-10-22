// BoardUI's Chip (github.com/BoardUI/boardui, MIT) re-expressed in Neko's tokens: a
// rounded-md label, 12px / 500, px-1.5, soft tinted fill + readable text. Colour comes
// from the semantic tones (warning, info, success, destructive, primary), never raw hex.
import * as React from "react";
import { cn } from "@/ui/lib/utils";

export type ChipTone = "attention" | "review" | "running" | "done" | "failed" | "neutral";

const TONES: Record<ChipTone, string> = {
  attention: "bg-warning/12 text-warning-foreground",
  review: "bg-info/12 text-info-foreground",
  running: "bg-primary/12 text-primary",
  done: "bg-success/12 text-success-foreground",
  failed: "bg-destructive/12 text-destructive-foreground",
  neutral: "bg-foreground/[0.06] text-foreground/70",
};

export function Chip({ tone = "neutral", className, ...props }: React.ComponentProps<"span"> & { tone?: ChipTone }) {
  return (
    <span
      data-slot="chip"
      data-tone={tone}
      className={cn(
        "inline-flex h-5 shrink-0 items-center gap-1 whitespace-nowrap rounded-md px-1.5 text-[12px] font-medium leading-none tracking-[0.01em] [&_svg]:size-3",
        TONES[tone], className,
      )}
      {...props}
    />
  );
}
