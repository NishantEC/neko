// from hushbacks packages/ui/src/components/editorial/SectionHead.tsx
import { cn } from "@/ui/lib/utils";
import type { ReactNode } from "react";

import { EditorialHeading } from "./EditorialHeading";
import { MonoLabel } from "./MonoLabel";

export interface SectionHeadProps {
  eyebrow?: ReactNode;
  title: ReactNode;
  intro?: ReactNode;
  align?: "left" | "center";
  className?: string;
}

export const SectionHead: React.FC<SectionHeadProps> = ({ eyebrow, title, intro, align = "left", className }) => (
  <header
    className={cn(
      "flex flex-col gap-4",
      align === "center" ? "items-center text-center" : "items-start text-left",
      className,
    )}
  >
    {eyebrow ? <MonoLabel>{eyebrow}</MonoLabel> : null}
    <EditorialHeading as="h2" className="text-[clamp(2.25rem,3.75vw+0.5rem,3.5rem)] leading-[1.1]">
      {title}
    </EditorialHeading>
    {intro ? (
      <p
        className={cn(
          "text-muted-foreground text-[17px] leading-[1.55] max-w-[60ch]",
          align === "center" ? "mx-auto" : "",
        )}
      >
        {intro}
      </p>
    ) : null}
  </header>
);
