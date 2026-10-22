// The small set of layout pieces every setup page shares, built only from the
// design system (@/ui, @/editorial): a page frame, sections that open
// eyebrow → heading → subtitle, one Panel divided into rows with a footer
// action bar, instrument wells, empty/loading/error states and a confirm.
import { useEffect, type ReactNode } from "react";
import { cn } from "@/ui/lib/utils";
import { Button } from "@/ui/button";
import { Alert, AlertDescription, AlertTitle } from "@/ui/alert";
import { Skeleton } from "@/ui/skeleton";
import { StatusBadge } from "@/ui/status-badge";
import { Switch } from "@/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui/select";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter,
  AlertDialogHeader, AlertDialogTitle,
} from "@/ui/alert-dialog";
import { WarningCircle, type Icon } from "@/ui/icons";
import { EditorialHeading, FieldLabel, PageHeader, Panel as BasePanel } from "@/editorial";
import { useWorkbench } from "@/useStore";
import type { Tone } from "@/setup/lib";

/** The page frame: header (eyebrow, serif title, subtitle) with page actions, then the content. */
export function SetupPage({
  eyebrow, title, subtitle, actions, children, width = "max-w-[880px]", toolbar,
}: {
  eyebrow: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  toolbar?: ReactNode;
  children: ReactNode;
  width?: string;
}) {
  return (
    <div data-slot="setup-page" className="min-h-0 flex-1 overflow-y-auto">
      <div className={cn("mx-auto flex w-full flex-col gap-8 px-6 pt-12 pb-20 md:px-10", width)}>
        <div className="flex flex-wrap items-end justify-between gap-4">
          <PageHeader eyebrow={eyebrow} title={title} subtitle={subtitle} />
          {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
        </div>
        {toolbar}
        {children}
      </div>
    </div>
  );
}

/** A group on a page: eyebrow → heading → intro, then its content. */
export function Section({
  title, intro, actions, children, id, className,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  intro?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  id?: string;
  className?: string;
}) {
  return (
    <section id={id} data-slot="setup-section" className={cn("flex flex-col gap-4", className)}>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1.5">
          <EditorialHeading as="h2" className="text-[clamp(1.25rem,1.2vw+0.9rem,1.625rem)]">{title}</EditorialHeading>
          {intro ? <p className="max-w-[62ch] text-[14px] leading-[1.55] text-muted-foreground">{intro}</p> : null}
        </div>
        {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
      </div>
      {children}
    </section>
  );
}

/**
 * The editorial Panel, with the attributes it would otherwise drop (`data-slot`,
 * aria-*) carried on a layout-free wrapper so checks and styles can find it.
 */
export function Panel({ children, className, ...rest }: React.ComponentProps<"div">) {
  return (
    <div {...rest} className="contents">
      <BasePanel className={className}>{children}</BasePanel>
    </div>
  );
}

/** A distinct footer action bar for a Panel. */
export function PanelFooter({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-wrap items-center justify-end gap-2 border-t border-border/70 bg-foreground/[0.02] px-6 py-4", className)}>
      {children}
    </div>
  );
}

/** A row: title and a muted line under it, with leading and trailing content. */
export function Row({
  title, description, leading, trailing, children, className, onClick, label, id, highlighted,
}: {
  title: ReactNode;
  description?: ReactNode;
  leading?: ReactNode;
  trailing?: ReactNode;
  children?: ReactNode;
  className?: string;
  onClick?: () => void;
  label?: string;
  id?: string;
  highlighted?: boolean;
}) {
  const head = (
    <>
      {leading ? <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center text-muted-foreground">{leading}</span> : null}
      <span className="flex min-w-0 flex-1 flex-col gap-0.5 text-start">
        <span className="text-[14px] font-medium leading-snug text-foreground">{title}</span>
        {description ? <span className="text-[13px] leading-[1.45] text-muted-foreground">{description}</span> : null}
      </span>
    </>
  );
  return (
    <div
      id={id}
      data-slot="setup-row"
      data-highlighted={highlighted ? "" : undefined}
      className={cn("min-h-14 px-6 py-4 transition-colors data-[highlighted]:bg-primary/10", className)}
    >
      <div className="flex items-start gap-3">
        {onClick ? (
          <button
            type="button"
            onClick={onClick}
            aria-label={label}
            className="-m-1.5 flex min-w-0 flex-1 items-start gap-3 rounded-lg p-1.5 text-start outline-none transition-colors hover:bg-foreground/[0.04] focus-visible:ring-2 focus-visible:ring-ring"
          >
            {head}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-start gap-3">{head}</div>
        )}
        {trailing ? <div className="flex shrink-0 items-center gap-2">{trailing}</div> : null}
      </div>
      {children ? <div className="mt-3 ps-8 text-[13px] text-muted-foreground">{children}</div> : null}
    </div>
  );
}

/**
 * A labelled control row, for settings. `id` lets search jump to it: the page
 * scrolls to `#setting-<id>` and lights it by setting `data-highlighted`.
 */
export function SettingRow({
  id, title, description, children, highlighted, stacked,
}: {
  id?: string;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  highlighted?: boolean;
  stacked?: boolean;
}) {
  return (
    <div
      id={id ? `setting-${id}` : undefined}
      data-slot="setting-row"
      data-setting={id}
      data-highlighted={highlighted ? "" : undefined}
      className={cn("scroll-mt-24 min-h-14 px-6 py-4 transition-colors duration-500 data-[highlighted]:bg-primary/10", stacked ? "flex flex-col gap-3" : "flex flex-wrap items-center justify-between gap-x-6 gap-y-3")}
    >
      <div className="flex min-w-0 max-w-[44ch] flex-col gap-0.5">
        <FieldLabel className="text-[11px]">{title}</FieldLabel>
        {description ? <p className="text-[13px] leading-[1.5] text-muted-foreground">{description}</p> : null}
      </div>
      <div className={cn("flex items-center gap-2", stacked && "w-full")}>{children}</div>
    </div>
  );
}

export function Status({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return (
    <StatusBadge tone={tone}>
      <span aria-hidden className={cn("size-1.5 rounded-full", tone === "success" ? "bg-success" : tone === "warning" ? "bg-warning" : tone === "error" ? "bg-destructive" : "bg-muted-foreground/60")} />
      {children}
    </StatusBadge>
  );
}

/** A switch with its name, so it is reachable and announced. */
export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (value: boolean) => void; label: string; disabled?: boolean }) {
  return <Switch checked={checked} onCheckedChange={(value) => onChange(Boolean(value))} aria-label={label} disabled={disabled} />;
}

export interface Option { value: string; label: string; hint?: string }

/** A select that shows the label of the chosen option, not its value. */
export function Pick({
  value, onChange, options, label, className, disabled, placeholder, id,
}: {
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  label: string;
  className?: string;
  disabled?: boolean;
  placeholder?: string;
  id?: string;
}) {
  const items = options.map((o) => ({ value: o.value, label: o.label }));
  return (
    <Select value={value} onValueChange={onChange} items={items} disabled={disabled}>
      <SelectTrigger id={id} aria-label={label} className={cn("min-w-44", className)} size="sm">
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
            {o.hint ? <span className="ms-2 text-muted-foreground">{o.hint}</span> : null}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** The daemon's own message, verbatim. */
export function ErrorNote({ error, title = "That didn’t work", onDismiss, className }: { error: string | null | undefined; title?: string; onDismiss?: () => void; className?: string }) {
  if (!error) return null;
  return (
    <Alert variant="destructive" data-slot="error-note" className={className}>
      <WarningCircle />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription className="break-words whitespace-pre-wrap select-text">{error}</AlertDescription>
      {onDismiss ? (
        <div className="col-start-2 mt-1">
          <Button variant="ghost" size="xs" onClick={onDismiss}>Dismiss</Button>
        </div>
      ) : null}
    </Alert>
  );
}

export function EmptyBlock({
  icon: IconComponent, title, description, children, className,
}: { icon?: Icon; title: ReactNode; description?: ReactNode; children?: ReactNode; className?: string }) {
  return (
    <Empty className={cn("border border-dashed border-border bg-card/40 p-10", className)}>
      <EmptyHeader>
        {IconComponent ? <EmptyMedia variant="icon"><IconComponent /></EmptyMedia> : null}
        <EmptyTitle>{title}</EmptyTitle>
        {description ? <EmptyDescription>{description}</EmptyDescription> : null}
      </EmptyHeader>
      {children}
    </Empty>
  );
}

export function LoadingRows({ rows = 3 }: { rows?: number }) {
  return (
    <Panel aria-busy="true" aria-label="Loading">
      <div className="divide-y divide-border/70">
        {Array.from({ length: rows }, (_, i) => (
          <div key={i} className="flex flex-col gap-2 px-6 py-4">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-3 w-80 max-w-full" />
          </div>
        ))}
      </div>
    </Panel>
  );
}

/** Shows the loading skeleton until the workbench has answered, or the connection error. */
export function useSnapshotGate() {
  const { snapshot, connected, error } = useWorkbench();
  return { snapshot, connected, error, ready: !!snapshot };
}

export function Gate({ children }: { children: (snapshot: any) => ReactNode }) {
  const { snapshot, error, connected } = useWorkbench();
  if (!snapshot) {
    return error && !connected ? (
      <ErrorNote title="Can’t reach the Neko daemon" error={error} />
    ) : (
      <LoadingRows rows={4} />
    );
  }
  return <>{children(snapshot)}</>;
}

/** A confirmation for something that can’t be undone. */
export function ConfirmDialog({
  open, onOpenChange, title, description, confirmLabel, onConfirm, destructive = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  destructive?: boolean;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant={destructive ? "destructive" : "default"} onClick={onConfirm}>{confirmLabel}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** An instrument well: a mono reading over a track, in a tone. */
export function Well({
  label, value, unit, fraction, tone = "neutral", detail, className,
}: {
  label: string;
  value: ReactNode;
  unit?: string;
  /** 0..1 fills the track; undefined shows no track. */
  fraction?: number;
  tone?: Tone;
  detail?: ReactNode;
  className?: string;
}) {
  const fill = tone === "error" ? "bg-destructive" : tone === "warning" ? "bg-warning" : tone === "success" ? "bg-success" : "bg-foreground/60";
  const pct = fraction === undefined ? undefined : Math.max(0, Math.min(1, fraction));
  return (
    <div
      data-slot="well"
      data-tone={tone}
      className={cn("flex flex-col gap-2.5 rounded-xl border border-border bg-foreground/[0.03] p-4 shadow-[inset_0_1px_2px_rgba(0,0,0,0.06)]", className)}
    >
      <FieldLabel>{label}</FieldLabel>
      <div className="flex items-baseline gap-1.5">
        <span className="font-mono text-2xl font-medium tabular-nums text-foreground">{value}</span>
        {unit ? <span className="font-mono text-xs text-muted-foreground">{unit}</span> : null}
      </div>
      {pct !== undefined ? (
        <div
          role="meter"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(pct * 100)}
          className="h-1.5 w-full overflow-hidden rounded-full bg-foreground/10"
        >
          <div className={cn("h-full rounded-full transition-[width] duration-500", fill)} style={{ width: `${pct * 100}%` }} />
        </div>
      ) : null}
      {detail ? <p className="text-xs leading-snug text-muted-foreground">{detail}</p> : null}
    </div>
  );
}

/** A labelled form control for dialogs. */
export function FormField({
  label, htmlFor, hint, children, className,
}: { label: ReactNode; htmlFor?: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">{label}</label>
      {children}
      {hint ? <p className="text-xs leading-snug text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

/** Ranked horizontal bars: a label, a mono value and a track, for short lists of comparable numbers. */
export function BarList({ items, label }: { items: { name: string; value: number; text: string; hint?: string }[]; label: string }) {
  const max = Math.max(1, ...items.map((i) => i.value));
  return (
    <ul aria-label={label} className="flex flex-col gap-4 px-6 py-5">
      {items.map((item) => (
        <li key={item.name} className="flex flex-col gap-1.5">
          <div className="flex items-baseline justify-between gap-4 text-[13px]">
            <span className="min-w-0 truncate text-foreground/90">{item.name}{item.hint ? <span className="ms-2 text-muted-foreground">{item.hint}</span> : null}</span>
            <span className="shrink-0 font-mono tabular-nums text-muted-foreground">{item.text}</span>
          </div>
          <div role="meter" aria-label={item.name} aria-valuemin={0} aria-valuemax={max} aria-valuenow={item.value} className="h-1.5 overflow-hidden rounded-full bg-foreground/10">
            <div className="h-full rounded-full bg-foreground/60 transition-[width] duration-500" style={{ width: `${(item.value / max) * 100}%` }} />
          </div>
        </li>
      ))}
    </ul>
  );
}

/** Adds ⌘K actions while a page is open (see shell/commands). */
export { registerCommands } from "@/shell/commands";

/** Focuses a field when a dialog opens. */
export function useAutofocus(ref: React.RefObject<HTMLElement | null>, active: boolean) {
  useEffect(() => {
    if (active) setTimeout(() => ref.current?.focus(), 50);
  }, [active, ref]);
}
