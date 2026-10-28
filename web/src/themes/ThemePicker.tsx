// A grid of theme cards for one appearance (light or dark). Each card draws a
// small mock window from the theme's own colours, so what you see is what you
// get without a screenshot to keep up to date.
import { useRef } from "react";
import { cn } from "@/ui/lib/utils";
import { Check, Trash, Warning } from "@/ui/icons";
import { Badge } from "@/ui/badge";
import { deriveVars, resolveColors } from "@/themes/derive";
import type { ThemeInfo } from "@/theme";

/** The mock window: sidebar, page, one card, two text lines, a primary button, status dots. */
export function ThemeSwatch({ theme, className }: { theme: ThemeInfo; className?: string }) {
  const c = resolveColors(theme.data);
  const wash = theme.data.backdrop ? deriveVars(theme.data).find(([name]) => name === "--backdrop-wash")?.[1] : undefined;
  const line = (width: string, color: string) => <span className="block h-[3px] rounded-full" style={{ width, background: color }} />;
  return (
    <span
      aria-hidden
      data-slot="theme-swatch"
      className={cn("relative flex h-[72px] w-full overflow-hidden rounded-lg border", className)}
      style={{ background: wash ?? c.background, borderColor: `color-mix(in srgb, ${c.foreground} 18%, transparent)` }}
    >
      <span className="flex w-[26%] flex-col gap-1.5 p-1.5" style={{ background: c.sidebar, opacity: theme.data.backdrop ? 0.7 : 1 }}>
        {line("70%", c.sidebarForeground)}
        {line("50%", c.mutedForeground)}
        {line("60%", c.mutedForeground)}
      </span>
      <span className="flex flex-1 flex-col gap-1.5 p-1.5">
        <span className="flex flex-col gap-1 rounded-[5px] p-1.5" style={{ background: c.card, opacity: theme.data.backdrop ? 0.85 : 1 }}>
          {line("80%", c.foreground)}
          {line("55%", c.mutedForeground)}
        </span>
        <span className="mt-auto flex items-center gap-1">
          <span className="h-2.5 w-7 rounded-full" style={{ background: c.primary }} />
          <span className="ms-auto size-1.5 rounded-full" style={{ background: c.success }} />
          <span className="size-1.5 rounded-full" style={{ background: c.warning }} />
          <span className="size-1.5 rounded-full" style={{ background: c.destructive }} />
        </span>
      </span>
    </span>
  );
}

export function ThemePicker({
  label, themes, value, onChange, onRemove,
}: {
  label: string;
  themes: ThemeInfo[];
  /** The id in use for this appearance. */
  value: string;
  onChange(id: string): void;
  onRemove(theme: ThemeInfo): void;
}) {
  const group = useRef<HTMLDivElement>(null);
  const selected = themes.some((t) => t.id === value) ? value : themes[0]?.id;

  function move(delta: number) {
    const index = themes.findIndex((t) => t.id === selected);
    const next = themes[(index + delta + themes.length) % themes.length];
    if (!next) return;
    onChange(next.id);
    requestAnimationFrame(() => group.current?.querySelector<HTMLElement>(`[data-theme-id="${next.id}"]`)?.focus());
  }

  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={label}
      data-slot="theme-grid"
      className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4"
      onKeyDown={(event) => {
        if (event.key === "ArrowRight" || event.key === "ArrowDown") { event.preventDefault(); move(1); }
        else if (event.key === "ArrowLeft" || event.key === "ArrowUp") { event.preventDefault(); move(-1); }
      }}
    >
      {themes.map((theme) => {
        const checked = theme.id === selected;
        return (
          <div key={theme.id} className="group/theme relative" data-source={theme.source}>
            <button
              type="button"
              role="radio"
              aria-checked={checked}
              tabIndex={checked ? 0 : -1}
              data-theme-id={theme.id}
              data-slot="theme-card"
              onClick={() => onChange(theme.id)}
              className={cn(
                "flex w-full flex-col gap-2 rounded-xl border bg-card p-2 text-left outline-none transition-colors",
                "hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring",
                checked ? "border-primary ring-1 ring-primary" : "border-border",
              )}
            >
              <ThemeSwatch theme={theme} />
              <span className="flex min-w-0 items-center gap-1.5 px-0.5">
                <span className="truncate text-[13px] font-medium text-strong">{theme.name}</span>
                {checked ? <Check size={14} weight="bold" className="ms-auto shrink-0 text-primary" aria-label="Selected" /> : null}
              </span>
              <span className="flex min-h-5 flex-wrap items-center gap-1 px-0.5">
                {theme.family ? <span className="text-[11.5px] text-muted-foreground">{theme.family}</span> : null}
                {theme.source === "user" ? <Badge variant="secondary" size="default" className="px-1.5 py-0 text-[10.5px]" data-slot="theme-yours">{theme.overrides ? "Yours · replaces built-in" : "Yours"}</Badge> : null}
                {theme.lowContrast.length ? (
                  <Badge variant="destructive" className="px-1.5 py-0 text-[10.5px]" data-slot="theme-warning" title={`Under 4.5:1: ${theme.lowContrast.join("; ")}`}>
                    <Warning size={11} /> Low contrast
                  </Badge>
                ) : null}
              </span>
            </button>
            {theme.source === "user" ? (
              <button
                type="button"
                data-slot="theme-remove"
                aria-label={`Remove ${theme.name}`}
                title={theme.overrides ? "Remove my file (the built-in comes back)" : "Remove this theme"}
                onClick={() => onRemove(theme)}
                className="absolute right-3.5 top-3.5 flex size-6 items-center justify-center rounded-md bg-background/80 text-muted-foreground opacity-0 outline-none backdrop-blur transition-opacity hover:text-destructive-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover/theme:opacity-100"
              >
                <Trash size={13} />
              </button>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
