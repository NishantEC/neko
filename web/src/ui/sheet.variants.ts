// from hushbacks packages/ui/src/components/sheet.variants.ts
import { cn } from "@/ui/lib/utils";

export type SheetVariant = "floating" | "flush";
export type SheetSide = "top" | "right" | "bottom" | "left";

const FLOATING_SURFACE =
  "z-50 flex flex-col gap-0 overflow-hidden rounded-[20px] bg-[color-mix(in_oklab,var(--foreground)_3%,var(--card))] text-sm text-foreground outline-none";

const FLOATING_SIDES: Record<SheetSide, string> = {
  right:
    "top-[max(0.5rem,env(safe-area-inset-top))] right-2 bottom-[max(0.5rem,env(safe-area-inset-bottom))] w-[calc(100%-1rem)] sm:max-w-sm shadow-[-28px_0_48px_-28px_rgb(0_0_0/0.28)]",
  left: "top-[max(0.5rem,env(safe-area-inset-top))] left-2 bottom-[max(0.5rem,env(safe-area-inset-bottom))] w-[calc(100%-1rem)] sm:max-w-sm shadow-[28px_0_48px_-28px_rgb(0_0_0/0.28)]",
  top: "inset-x-2 top-[max(0.5rem,env(safe-area-inset-top))] max-h-[calc(100dvh-1rem)] shadow-[0_28px_48px_-28px_rgb(0_0_0/0.28)]",
  bottom:
    "inset-x-2 bottom-[max(0.5rem,env(safe-area-inset-bottom))] max-h-[calc(100dvh-1rem)] shadow-[0_-28px_48px_-28px_rgb(0_0_0/0.28)]",
};

const CONTAINED_SIDES: Record<SheetSide, string> = {
  right: "top-2 bottom-2",
  left: "top-2 bottom-2",
  top: "top-2",
  bottom: "bottom-2",
};

const FLUSH_SURFACE = [
  "fixed z-50 flex flex-col gap-4 bg-background bg-clip-padding text-sm shadow-lg outline-none transition-transform duration-300 ease-in-out",
  "data-[side=right]:inset-y-0 data-[side=right]:right-0 data-[side=right]:h-full data-[side=right]:w-3/4 data-[side=right]:sm:max-w-sm data-[side=right]:data-starting-style:translate-x-full data-[side=right]:data-ending-style:translate-x-full",
  "data-[side=left]:inset-y-0 data-[side=left]:left-0 data-[side=left]:h-full data-[side=left]:w-3/4 data-[side=left]:sm:max-w-sm data-[side=left]:data-starting-style:-translate-x-full data-[side=left]:data-ending-style:-translate-x-full",
  "data-[side=top]:inset-x-0 data-[side=top]:top-0 data-[side=top]:h-auto data-[side=top]:data-starting-style:-translate-y-full data-[side=top]:data-ending-style:-translate-y-full",
  "data-[side=bottom]:inset-x-0 data-[side=bottom]:bottom-0 data-[side=bottom]:h-auto data-[side=bottom]:data-starting-style:translate-y-full data-[side=bottom]:data-ending-style:translate-y-full",
].join(" ");

export const SHEET_OVERLAY: Record<SheetVariant, string> = {
  floating: "inset-0 z-50 bg-black/[0.06] dark:bg-black/30",
  flush: "fixed inset-0 z-50 bg-black/10 supports-backdrop-filter:backdrop-blur-xs",
};

export function sheetSurfaceClass(variant: SheetVariant, side: SheetSide, contained: boolean) {
  if (variant === "flush") return FLUSH_SURFACE;
  return cn(
    FLOATING_SURFACE,
    contained ? "absolute" : "fixed",
    FLOATING_SIDES[side],
    contained && CONTAINED_SIDES[side],
  );
}
