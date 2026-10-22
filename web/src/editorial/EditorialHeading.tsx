// from hushbacks packages/ui/src/components/editorial/EditorialHeading.tsx
import { cn } from "@/ui/lib/utils";
import type { CSSProperties, ElementType, ReactNode } from "react";

export interface EditorialHeadingProps {
  as?: ElementType;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

const FRAUNCES_STYLE: CSSProperties = {
  fontFamily: 'var(--font-serif, "Fraunces", Georgia, serif)',
  fontOpticalSizing: "auto",
  fontVariationSettings: '"opsz" 144',
  fontWeight: 500,
};

export const EditorialHeading: React.FC<EditorialHeadingProps> = ({ as: Tag = "h1", children, className, style }) => (
  <Tag
    className={cn(
      "text-foreground tracking-[-0.015em] leading-[1.08]",
      "[&_em]:italic [&_em]:font-normal [&_em]:text-foreground/60",
      className,
    )}
    style={{ ...FRAUNCES_STYLE, ...style }}
  >
    {children}
  </Tag>
);
