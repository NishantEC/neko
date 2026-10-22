// from hushbacks packages/ui/src/components/editorial/GlowTile.tsx
import { cn } from "@/ui/lib/utils";
import type { CSSProperties, ReactNode } from "react";

export type GlowCorner = "tl" | "tr" | "bl" | "br";

export interface GlowTileProps {
  /** Tile frame classes (grid spans, min-height, base border). */
  containerClassName?: string;
  /** Padded content wrapper classes. */
  className?: string;
  /** CSS color for the corner glow (e.g. `oklch(54.6% 0.245 262 / 0.10)`). */
  glowColor?: string;
  /** Corner the glow emits from. Default `tr`. */
  glowCorner?: GlowCorner;
  /** Optional click handler — when set, tile gets cursor + focus styles. */
  onClick?: () => void;
  children?: ReactNode;
}

const CORNER_STYLES: Record<GlowCorner, CSSProperties> = {
  tl: { top: "-40%", left: "-20%" },
  tr: { top: "-40%", right: "-20%" },
  bl: { bottom: "-40%", left: "-20%" },
  br: { bottom: "-40%", right: "-20%" },
};

/**
 * GlowTile — surface card with an optional corner radial glow.
 *
 * Theme-agnostic: uses `bg-card` + `border-border` so it works in both
 * light and dark modes. The glow is a single absolute-positioned span
 * with a `radial-gradient`, kept low-opacity by default so the same
 * component reads as warm in light mode and luminous in dark mode.
 */
export const GlowTile: React.FC<GlowTileProps> = ({
  containerClassName,
  className,
  glowColor,
  glowCorner = "tr",
  onClick,
  children,
}) => {
  const Tag = onClick ? "button" : "div";
  return (
    <Tag
      type={onClick ? "button" : undefined}
      onClick={onClick}
      className={cn(
        "group relative overflow-hidden rounded-2xl border border-border/60 bg-card text-left",
        onClick
          ? cn(
              "cursor-pointer transition-[transform,border-color,box-shadow] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]",
              "hover:-translate-y-0.5 hover:border-primary/30 hover:shadow-[0_12px_32px_-16px_rgba(0,0,0,0.35)]",
              "active:translate-y-0 active:scale-[0.995] active:duration-100",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
              "motion-reduce:transform-none motion-reduce:transition-none",
            )
          : "transition-colors duration-150",
        containerClassName,
      )}
    >
      {glowColor ? (
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute h-[80%] w-[90%] rounded-full blur-3xl",
            onClick &&
              "opacity-70 transition-[opacity,transform] duration-500 ease-out group-hover:scale-110 group-hover:opacity-100 motion-reduce:transition-none motion-reduce:group-hover:scale-100",
          )}
          style={{
            ...CORNER_STYLES[glowCorner],
            background: `radial-gradient(closest-side, ${glowColor}, transparent 75%)`,
          }}
        />
      ) : null}
      <div className={cn("relative h-full w-full", className)}>{children}</div>
    </Tag>
  );
};
