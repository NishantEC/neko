// from hushbacks packages/ui/src/components/editorial/EditorialHeading.tsx
import { cn } from "@/ui/lib/utils";
import type { CSSProperties, ElementType, ReactNode } from "react";

export interface EditorialHeadingProps {
  as?: ElementType;
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
}

const PLAIN_STYLE: CSSProperties = { fontWeight: 600 };

export const EditorialHeading: React.FC<EditorialHeadingProps> = ({ as: Tag = "h1", children, className, style }) => (
  <Tag
    className={cn(
      "text-foreground tracking-[-0.01em] leading-[1.2]",
      "[&_em]:not-italic [&_em]:font-[inherit]",
      className,
    )}
    style={{ ...PLAIN_STYLE, ...style }}
  >
    {children}
  </Tag>
);
