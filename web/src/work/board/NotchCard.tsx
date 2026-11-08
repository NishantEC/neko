// The task card: one quiet rounded surface whose last line is the state (DESIGN.md §8).
// The status glyph says where the task stands; no frame, no notch, no coloured words.
// When the task waits on you, its one action (Start again, Answer, Approve) sits at the
// end of the state line and shows on hover or focus, like a row's actions; it is also in
// the card's right-click menu. Done cards end on their meta line. Drawn once here for the
// board (BoardCard) and the gallery's specimens, so the two can never drift.
//
// History: until 2026-11-08 a waiting card wore a frame whose material flowed into a notch
// at the bottom-right holding the action (the "ticket with a bite taken out"). §8 retired
// it: a border and a permanent button on every waiting card was the noise the owner named.
import type { CSSProperties, ReactNode } from "react";
import { GitPullRequest, type Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { TONE_DOT, type Tone } from "@/work/tone";
import { UnseenDot } from "@/work/seen";

export type Density = "comfortable" | "compact" | "dense";

export interface NotchAction {
  label: string;
  icon: Icon;
  onClick: () => void;
}

export interface NotchCardProps {
  tone: Tone;
  title: string;
  /** The state line. Null: a done card, which ends on its meta line. */
  state: { label: string; age?: string; detail?: string } | null;
  /** Running: the step and its clock replace the state, the step shimmering. */
  live?: { step: string; clock: string } | null;
  reason?: string | null;
  /** The well's meta line (comfortable), and a done card's last line at every density. */
  meta?: ReactNode;
  /** A pull request number, at the end of the state line. */
  pr?: string | number | null;
  /** Checks on that pull request, as a dot on its mark. */
  prChecks?: "passing" | "failing" | null;
  /** The card's one action, shown on hover or focus at the end of the state line. */
  action?: NotchAction | null;
  density?: Density;
  /** Leads the state line: the status glyph. */
  notchLead?: ReactNode;
  /** Revealed at the top-right corner on hover or focus. */
  corner?: ReactNode;
  /** Finished since the person last opened it: a small Iris dot at the top-right (work/seen.tsx). */
  unseen?: boolean;
  selected?: boolean;
  /** The placeholder left behind while the card is carried. */
  ghost?: boolean;
  /** The copy that follows the pointer. */
  lifted?: boolean;
  /** The focusable hit area, drawn under the content (absolute, full size). */
  handle?: ReactNode;
  className?: string;
  style?: CSSProperties;
  [data: `data-${string}`]: string | undefined;
}

const DENSITY = {
  comfortable: { pad: "p-4 pb-2.5", gap: "gap-2.5", title: "text-[15px] line-clamp-2", row: 40, text: "text-[12.5px]", act: "h-7 px-2.5 text-[12.5px]", icon: 13 },
  compact: { pad: "px-3 pt-3 pb-2", gap: "gap-1.5", title: "text-[14px] line-clamp-2", row: 26, text: "text-[12px]", act: "h-[22px] px-2 text-[12px]", icon: 12 },
  dense: { pad: "px-3 pt-2.5 pb-1.5", gap: "gap-1", title: "text-[14px] truncate", row: 26, text: "text-[12px]", act: "h-[22px] px-2 text-[12px]", icon: 12 },
} as const;

function NotchState({ state, live, tone, lead }: Pick<NotchCardProps, "state" | "live" | "tone"> & { lead?: ReactNode }) {
  if (live) {
    return (
      <span className="flex min-w-0 items-center gap-1.5">
        {lead}
        <span className="min-w-0 truncate tabular-nums">
          <span className="bg-[linear-gradient(90deg,var(--muted-foreground)_35%,var(--foreground)_50%,var(--muted-foreground)_65%)] bg-[length:200%_100%] bg-clip-text text-transparent animate-shimmer-text motion-reduce:animate-none motion-reduce:text-muted-foreground">
            {live.step}
          </span>
          <span className="text-muted-foreground"> · {live.clock}</span>
        </span>
      </span>
    );
  }
  if (!state) return null;
  // When the row is tight the detail truncates first; then the age wraps onto a second line
  // that is clipped (so it hides whole, never half a number); then the label truncates.
  // Colour is the glyph's alone (DESIGN.md §2, §3): the words stay muted.
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {lead ?? <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />}
      <span data-slot="card-state" className="flex max-h-[1lh] min-w-0 shrink flex-wrap items-center gap-x-1.5 overflow-hidden">
        <span className="min-w-0 max-w-full truncate text-foreground/75">{state.label}</span>
        {state.age ? <span data-slot="card-age" className="shrink-0 tabular-nums text-muted-foreground">{state.age}</span> : null}
      </span>
      {state.detail ? <span className="min-w-0 shrink-[999] truncate text-muted-foreground">· {state.detail}</span> : null}
    </span>
  );
}

function PrMark({ n, checks }: { n: string | number; checks?: "passing" | "failing" | null }) {
  return (
    <span className="relative inline-flex shrink-0 items-center gap-1 tabular-nums text-muted-foreground">
      <GitPullRequest size={12} aria-hidden />{n}
      {checks ? (
        <span
          role="img"
          aria-label={checks === "passing" ? "checks passing" : "checks failing"}
          className={cn("ms-0.5 size-1.5 rounded-full", checks === "passing" ? "bg-success" : "bg-destructive")}
        />
      ) : null}
    </span>
  );
}

/**
 * The card's inside: the title (and why it waits), then the state line. The action is laid
 * over the end of the state line on the surface's own colour, invisible until the card is
 * hovered or holds focus, so showing it never moves anything.
 */
function Body(props: NotchCardProps) {
  const { title, state, live, reason, meta, pr, prChecks, action, tone, notchLead, unseen } = props;
  const d = DENSITY[props.density ?? "compact"];
  const done = !(state || live);
  const acts = !done && !!action;
  return (
    <div data-slot="card-surface" className={cn("flex min-w-0 flex-col rounded-[14.5px] bg-[var(--fuse)] transition-colors duration-150", d.pad, d.gap, done && "pb-3")}>
      <span className={cn("font-medium leading-[1.4] text-strong", d.title, unseen && "pe-3")}>{title}</span>
      {reason && props.density !== "dense" ? (
        <span className={cn("text-[13px] leading-[1.45] text-foreground/70", props.density === "compact" && "truncate text-[12.5px]")}>{reason}</span>
      ) : null}
      {meta && (props.density === "comfortable" || done) ? (
        <span className={cn("flex min-w-0 items-center gap-2 text-muted-foreground", d.text)}>
          <span className="min-w-0 flex-1 truncate">{meta}</span>
          {pr ? <PrMark n={pr} checks={prChecks} /> : null}
        </span>
      ) : null}
      {done ? null : (
        <span data-slot="notch-row" className={cn("relative -mb-0.5 flex min-w-0 items-center justify-between gap-2", d.text)} style={{ minHeight: d.row - 4 }}>
          <span data-slot={live ? "card-live" : "card-tab"} className="flex min-w-0 items-center">
            <NotchState state={state} live={live} tone={tone} lead={notchLead} />
          </span>
          {pr ? <PrMark n={pr} checks={prChecks} /> : null}
          {acts ? (
            <button
              type="button"
              data-slot="card-action"
              onClick={(e) => { e.stopPropagation(); action!.onClick(); }}
              onPointerDown={(e) => e.stopPropagation()}
              onKeyDown={(e) => e.stopPropagation()}
              className={cn(
                "pointer-events-auto absolute end-[-4px] top-1/2 z-10 inline-flex -translate-y-1/2 cursor-pointer items-center gap-1.5 rounded-full bg-[var(--fuse)] font-medium leading-none text-foreground shadow-[-10px_0_8px_var(--fuse)]",
                "opacity-0 transition-[opacity,background-color] duration-150 group-hover/card:opacity-100 group-focus-within/card:opacity-100 group-data-[selected]/card:opacity-100 [@media(hover:none)]:opacity-100 motion-reduce:transition-none",
                "hover:bg-[color-mix(in_oklab,var(--foreground)_8%,var(--fuse))] active:bg-[color-mix(in_oklab,var(--foreground)_12%,var(--fuse))] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                d.act,
              )}
            >
              <action.icon size={d.icon} weight="bold" aria-hidden />{action!.label}
            </button>
          ) : null}
        </span>
      )}
    </div>
  );
}

/**
 * The card: a rounded surface (`--fuse`) on the column, no frame. Hover lifts the surface a
 * step; selected (the open task) wears a primary ring, the one place colour marks the card;
 * keyboard focus adds the focus ring. Nothing moves.
 */
export function NotchCard(props: NotchCardProps) {
  const { selected, ghost, lifted, handle, corner, unseen, className, style, density = "compact", ...rest } = props;
  const data = Object.fromEntries(Object.entries(rest).filter(([k]) => k.startsWith("data-")));
  return (
    <div
      {...data}
      data-density={density}
      data-selected={selected ? "" : undefined}
      data-unseen={unseen ? "" : undefined}
      style={style}
      className={cn(
        "group/card relative flex w-full flex-col rounded-[16px] p-[1.5px] text-start transition-[background-color,box-shadow] duration-150",
        "[--fuse-base:var(--card)] dark:[--fuse-base:var(--sky-pebble)] [--fuse:var(--fuse-base)] [--frame:var(--fuse)]",
        !ghost && !lifted && !selected && "hover:[--fuse:color-mix(in_oklab,var(--foreground)_4%,var(--fuse-base))]",
        selected && "[--frame:var(--primary)] [--fuse:color-mix(in_oklab,var(--primary)_6%,var(--fuse-base))]",
        "bg-[var(--frame)]",
        "has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring has-[:focus-visible]:ring-offset-1 has-[:focus-visible]:ring-offset-background",
        lifted && "shadow-[0_10px_24px_-6px_rgb(0_0_0/0.28)]",
        ghost && "opacity-40",
        className,
      )}
    >
      {handle}
      {/* positioned, so it paints above the handle; the action re-enables its own pointer events */}
      <div className="pointer-events-none relative">
        <Body {...props} unseen={unseen} density={density} />
        {unseen ? <UnseenDot className="absolute top-[13px] end-[11px]" /> : null}
      </div>
      {corner ? (
        <span className="pointer-events-none absolute top-2 end-2 z-10 opacity-0 transition-opacity duration-150 group-hover/card:pointer-events-auto group-hover/card:opacity-100 group-has-[:focus-visible]/card:pointer-events-auto group-has-[:focus-visible]/card:opacity-100 motion-reduce:transition-none">
          {corner}
        </span>
      ) : null}
    </div>
  );
}

/** "18m", "6h", "3d": an age as short as it reads. */
export function shortAge(ms: number, now = Date.now()): string {
  if (!ms || ms <= 0) return "";
  const m = Math.max(0, Math.round((now - ms) / 60000));
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  if (m < 1440) return `${Math.round(m / 60)}h`;
  return `${Math.round(m / 1440)}d`;
}

/** "2m 08s": a running clock. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s >= 3600) return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
  return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}
