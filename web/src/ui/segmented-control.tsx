// BoardUI's Segmented Control (github.com/BoardUI/boardui, MIT): a recessed track with a
// raised thumb behind the selected segment that slides between segments. BoardUI builds it
// on React Aria; Neko's primitives are Base UI, so this is the same look on Base UI's
// ToggleGroup (single selection that cannot be emptied, roving arrow keys).
import { Toggle } from "@base-ui/react/toggle";
import { ToggleGroup } from "@base-ui/react/toggle-group";
import * as React from "react";
import { cn } from "@/ui/lib/utils";

export interface SegmentedControlProps {
  value: string;
  onValueChange: (value: string) => void;
  "aria-label": string;
  className?: string;
  children: React.ReactNode;
}

export function SegmentedControl({ value, onValueChange, className, children, ...rest }: SegmentedControlProps) {
  const track = React.useRef<HTMLDivElement>(null);
  const [thumb, setThumb] = React.useState<{ left: number; width: number } | null>(null);
  const [ready, setReady] = React.useState(false);

  React.useLayoutEffect(() => {
    const el = track.current;
    if (!el) return;
    const measure = () => {
      const on = el.querySelector<HTMLElement>("[data-pressed]");
      if (on) setThumb({ left: on.offsetLeft, width: on.offsetWidth });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    el.querySelectorAll("button").forEach((b) => ro.observe(b));
    // The first paint places the thumb without sliding in from nowhere.
    const frame = requestAnimationFrame(() => setReady(true));
    return () => { ro.disconnect(); cancelAnimationFrame(frame); };
  }, [value]);

  return (
    <ToggleGroup
      ref={track}
      data-slot="segmented-control"
      value={[value]}
      onValueChange={(vals) => { const next = vals[0]; if (next && next !== value) onValueChange(next); }}
      className={cn("relative inline-flex items-center gap-0.5 rounded-[10px] bg-foreground/[0.06] p-1", className)}
      {...rest}
    >
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-y-1 start-0 rounded-md bg-card shadow-xs ring-1 ring-foreground/[0.06]",
          "motion-reduce:transition-none",
          ready && "transition-[transform,width] duration-200 ease-[var(--ease-out)]",
        )}
        style={thumb ? { width: thumb.width, transform: `translateX(${thumb.left}px)` } : { opacity: 0 }}
      />
      {children}
    </ToggleGroup>
  );
}

export function SegmentedControlItem({ className, ...props }: React.ComponentProps<typeof Toggle>) {
  return (
    <Toggle
      data-slot="segmented-control-item"
      className={cn(
        "relative z-10 inline-flex h-6 cursor-pointer items-center justify-center gap-1.5 rounded-md px-2.5 text-[13px] font-normal text-muted-foreground outline-none transition-colors duration-150",
        "hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring data-pressed:font-medium data-pressed:text-strong [&_svg]:size-3.5",
        className,
      )}
      {...props}
    />
  );
}
