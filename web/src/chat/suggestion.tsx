// from hushbacks packages/ui/src/components/ai-elements/suggestion.tsx
import { StarFour } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import type { ComponentProps } from "react";

export type SuggestionsProps = ComponentProps<"div">;

export const Suggestions = ({ className, children, ...props }: SuggestionsProps) => (
  <div className={cn("flex flex-wrap gap-1.5", className)} {...props}>
    {children}
  </div>
);

export type SuggestionProps = {
  suggestion: string;
  onClick?: (suggestion: string) => void;
  className?: string;
  disabled?: boolean;
};

export const Suggestion = ({ suggestion, onClick, className, disabled }: SuggestionProps) => {
  const handleClick = () => {
    if (!disabled) {
      onClick?.(suggestion);
    }
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={disabled}
      className={cn(
        "group inline-flex items-center gap-1.5 rounded-[10px] border border-border/60 bg-foreground/[0.035] px-2.5 py-1.5",
        "text-[12.5px] font-medium text-foreground/80 text-left transition-colors duration-150",
        "hover:border-border hover:bg-foreground/[0.07] hover:text-foreground",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        "disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
    >
      <StarFour
        weight="fill"
        className="h-3 w-3 shrink-0 text-sky-500 transition-transform duration-200 ease-out group-hover:rotate-90 group-hover:scale-110 motion-reduce:transform-none"
      />
      <span className="whitespace-normal">{suggestion}</span>
    </button>
  );
};
