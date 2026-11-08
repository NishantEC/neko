// The small set of layout pieces every setup page shares. The frame is
// ui-kit' PageContainer (shell/PageContainer.tsx); Section, Panel, Row and the
// footer strip follow ui-kit' settings layout (apps/spa/src/pages/Settings/
// settings.ui.tsx @da20dfaeb), kept on Neko's props so the pages are unchanged.
// Also: instrument wells, empty/loading/error states and a confirm.
import { useEffect, useState, type ReactNode } from "react";
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
import { CaretDown, WarningCircle, type Icon } from "@/ui/icons";
import { FieldLabel, PageHeader } from "@/editorial";
import { PageContainer } from "@/shell/PageContainer";
import { useWorkbench } from "@/useStore";
import type { Tone } from "@/setup/lib";

/**
 * The page frame: ui-kit' PageContainer column (max 1200px, px-7, pt-6, pb-24: the Tasks page's rhythm) with its PageHeader and the page actions beside it.
 * `width` narrows the column for pages that are a single list.
 *
 * DESIGN.md §8: the header is the title and an inline `count`, nothing under
 * it; `actions` holds at most one primary button and an overflow menu.
 */
export function SetupPage({
  title, count, actions, children, width = "max-w-[880px]", toolbar,
}: {
  /** Ignored (no eyebrows); kept so call sites and plugins still type-check. */
  eyebrow?: ReactNode;
  title: ReactNode;
  count?: ReactNode;
  actions?: ReactNode;
  toolbar?: ReactNode;
  children: ReactNode;
  width?: string;
}) {
  return (
    <PageContainer data-slot="setup-page" className={cn("flex flex-col gap-8", width)}>
      <div className="flex min-h-9 flex-wrap items-center justify-between gap-4">
        <PageHeader title={title} count={count} />
        {actions ? <div className="flex flex-wrap items-center gap-1.5">{actions}</div> : null}
      </div>
      {toolbar}
      {children}
    </PageContainer>
  );
}

/** The quiet group heading every list shares (DESIGN.md §8): small, muted, a faint count. */
export const GROUP_HEADING = "text-[12.5px] font-medium text-muted-foreground";

/**
 * A group on a page: a small, quiet heading with an optional faint count and
 * actions on the right, then its rows. No intro sentence (DESIGN.md §8); a
 * group can collapse (`collapsible`, open by default).
 */
export function Section({
  title, count, actions, children, id, className, collapsible = false, defaultOpen = true,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  count?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  id?: string;
  className?: string;
  collapsible?: boolean;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const shown = !collapsible || open;
  const label = (
    <>
      <span className="text-balance">{title}</span>
      {count !== undefined && count !== null && count !== "" ? <span className="tabular-nums text-muted-foreground/70">{count}</span> : null}
    </>
  );
  return (
    <section id={id} data-slot="setup-section" data-open={shown || undefined} className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex min-h-7 items-center justify-between gap-4">
        <h2 className={cn(GROUP_HEADING, "min-w-0")}>
          {collapsible ? (
            <button
              type="button"
              aria-expanded={open}
              onClick={() => setOpen(!open)}
              className="-ms-1 flex items-center gap-1.5 rounded-md px-1 py-0.5 outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <CaretDown aria-hidden size={11} weight="bold" className={cn("transition-transform duration-150", !open && "-rotate-90")} />
              {label}
            </button>
          ) : (
            <span className="flex items-center gap-1.5">{label}</span>
          )}
        </h2>
        {actions ? <div className="flex shrink-0 flex-wrap items-center gap-1.5">{actions}</div> : null}
      </div>
      {shown ? children : null}
    </section>
  );
}

/**
 * A list's frame. No card, no border, no dividers (DESIGN.md §8): rows are
 * separated by space and light up on hover. It bleeds 12px either side so a
 * row's text lines up with the heading above while its hover wash has room.
 */
export function Panel({ children, className, ...rest }: React.ComponentProps<"div">) {
  return (
    <div data-slot="setup-panel" {...rest} className={cn("-mx-3 flex flex-col", className)}>
      {children}
    </div>
  );
}

/** A row of quiet actions under a list: a hint on the left, the action on the right. */
export function PanelFooter({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      data-slot="setup-panel-footer"
      className={cn("flex min-h-10 flex-wrap items-center justify-end gap-x-4 gap-y-2 px-3 pt-2 text-[13px] text-muted-foreground", className)}
    >
      {children}
    </div>
  );
}

/** A row: title and a muted line under it (settings.ui.tsx `Row`), with leading and trailing content. */
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
      {leading ? <span className="flex size-5 shrink-0 items-center justify-center text-muted-foreground">{leading}</span> : null}
      <span className="flex min-w-0 flex-1 items-baseline gap-3 text-start">
        <span className="min-w-0 shrink-0 truncate text-[13.5px] text-foreground">{title}</span>
        {description ? <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground">{description}</span> : null}
      </span>
    </>
  );
  return (
    <div
      id={id}
      data-slot="setup-row"
      data-highlighted={highlighted ? "" : undefined}
      className={cn("group/row rounded-[10px] px-3 py-2 transition-colors hover:bg-foreground/[0.03] data-[highlighted]:bg-primary/10", className)}
    >
      <div className="flex min-h-7 items-center gap-3">
        {onClick ? (
          <button
            type="button"
            onClick={onClick}
            aria-label={label}
            className="flex min-w-0 flex-1 items-center gap-3 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {head}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-3">{head}</div>
        )}
        {trailing ? <div className="flex shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground">{trailing}</div> : null}
      </div>
      {children ? <div className="mt-1.5 ps-8 text-[13px] text-muted-foreground">{children}</div> : null}
    </div>
  );
}

/**
 * A labelled control row (settings.ui.tsx `Row`): label and hint on the left,
 * the control on the right. `id` lets search jump to it: the page scrolls to
 * `#setting-<id>` and lights it by setting `data-highlighted`.
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
      className={cn(
        "flex min-h-12 scroll-mt-24 rounded-[10px] px-3 py-2.5 transition-colors duration-500 data-[highlighted]:bg-primary/10",
        stacked ? "flex-col gap-2" : "flex-col gap-2 sm:flex-row sm:items-center sm:justify-between sm:gap-4",
      )}
    >
      <div className="min-w-0">
        <div className="text-[13.5px] text-foreground">{title}</div>
        {description ? <div className="mt-0.5 max-w-prose text-pretty text-[12.5px] text-muted-foreground">{description}</div> : null}
      </div>
      <div className={cn("flex min-w-0 flex-wrap items-center gap-2", stacked ? "w-full" : "sm:shrink-0 sm:justify-end")}>{children}</div>
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

/**
 * A row's secondary actions (DESIGN.md §8): small icon buttons at its right
 * that appear while the row is hovered or holds focus (keyboard users reach
 * them with Tab; touch screens always see them). Every one of them is also in
 * the row's right-click menu. The row needs `group/row`.
 */
export function RowActions({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      data-slot="row-actions"
      className={cn(
        "flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-150 group-hover/row:opacity-100 group-focus-within/row:opacity-100 [@media(hover:none)]:opacity-100",
        className,
      )}
    >
      {children}
    </span>
  );
}

/** One icon button in `RowActions`: named, with its name as a tooltip. */
export function RowAction({ label, icon: IconComponent, onClick, href, disabled }: { label: string; icon: Icon; onClick?: () => void; href?: string; disabled?: boolean }) {
  const cls = "flex size-7 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-foreground/[0.07] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/70 disabled:pointer-events-none disabled:opacity-40";
  if (href) {
    return (
      <a href={href} target="_blank" rel="noreferrer" aria-label={label} title={label} className={cls} onClick={(e) => e.stopPropagation()}>
        <IconComponent aria-hidden size={15} />
      </a>
    );
  }
  return (
    <button type="button" aria-label={label} title={label} disabled={disabled} className={cls} onClick={(e) => { e.stopPropagation(); onClick?.(); }}>
      <IconComponent aria-hidden size={15} />
    </button>
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
    <Empty className={cn("border-0 px-6 py-12", className)}>
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
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-center gap-3 px-3 py-2.5">
          <Skeleton className="size-3.5 rounded-full" />
          <Skeleton className="h-4" style={{ width: `${32 + ((i * 17) % 36)}%` }} />
        </div>
      ))}
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
      className={cn("flex flex-col gap-2.5 rounded-xl bg-foreground/[0.03] p-4", className)}
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

/**
 * A figure as one calm line (DESIGN.md §8): its label, the value in tabular figures, then a muted
 * detail. No box and no track; colour only on a value that needs the person (warning, error),
 * always beside words that say why. The calm replacement for `Well` on list pages.
 */
export function Figure({
  label, value, unit, detail, tone = "neutral", title, className,
}: {
  label: ReactNode;
  value: ReactNode;
  unit?: ReactNode;
  detail?: ReactNode;
  tone?: Tone;
  /** The detail in full, on hover. */
  title?: string;
  className?: string;
}) {
  const color = tone === "error" ? "text-destructive-foreground" : tone === "warning" ? "text-warning-foreground" : "text-foreground";
  return (
    <div data-slot="figure" data-tone={tone} title={title} className={cn("flex min-h-9 items-baseline gap-3 rounded-[10px] px-3 py-2 transition-colors hover:bg-foreground/[0.03]", className)}>
      <span className="w-40 shrink-0 truncate text-[13.5px] text-foreground">{label}</span>
      <span data-slot="figure-value" className={cn("shrink-0 text-[13.5px] tabular-nums", color)}>
        {value}
        {unit ? <span className="ms-1 text-[12.5px] text-muted-foreground">{unit}</span> : null}
      </span>
      {detail ? <span className="min-w-0 flex-1 truncate text-[12.5px] text-muted-foreground">{detail}</span> : null}
    </div>
  );
}

/** A labelled form control for dialogs. */
export function FormField({
  label, htmlFor, hint, children, className,
}: { label: ReactNode; htmlFor?: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="text-[12.5px] font-medium text-muted-foreground">{label}</label>
      {children}
      {hint ? <p className="text-xs leading-snug text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

/** Ranked horizontal bars: a label, a mono value and a track, for short lists of comparable numbers. */
export function BarList({ items, label }: { items: { name: string; value: number; text: string; hint?: string }[]; label: string }) {
  const max = Math.max(1, ...items.map((i) => i.value));
  return (
    <ul aria-label={label} className="flex flex-col gap-4 px-3 py-3">
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
