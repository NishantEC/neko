// The task card: a thin frame around a rounded rectangle, and inside it a surface whose
// last line is the state. When the task waits on you, the surface leaves a notch at the
// bottom-right that the frame flows in to fill, and the action sits there on the frame
// colour, fused with the border (the owner's "ticket with a bite taken out", the bite the
// frame's own material). No action, no notch: just the frame. Done cards end on their meta
// line. Drawn once here for the board (BoardCard) and the gallery's specimens, so the two
// can never drift.
import type { CSSProperties, ReactNode } from "react";
import { GitPullRequest, type Icon } from "@/ui/icons";
import { cn } from "@/ui/lib/utils";
import { TONE_DOT, TONE_TEXT, type Tone } from "@/work/tone";

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
  /** A pull request number, at the end of the state line when there is no action. */
  pr?: string | number | null;
  /** Checks on that pull request, as a dot on its mark. */
  prChecks?: "passing" | "failing" | null;
  action?: NotchAction | null;
  density?: Density;
  /** Leads the state line: a source glyph, an agent's mark. */
  notchLead?: ReactNode;
  /** Revealed at the top-right corner on hover or focus. */
  corner?: ReactNode;
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
  comfortable: { pad: "p-4 pb-2.5", gap: "gap-2.5", title: "text-[15px] line-clamp-2", row: 40, r: 12, text: "text-[12.5px]", act: "h-7 px-2.5 text-[12.5px]", icon: 13 },
  compact: { pad: "px-3 pt-3 pb-2", gap: "gap-1.5", title: "text-[14px] line-clamp-2", row: 26, r: 10, text: "text-[12px]", act: "h-[22px] px-2 text-[12px]", icon: 12 },
  dense: { pad: "px-3 pt-2.5 pb-1.5", gap: "gap-1", title: "text-[14px] truncate", row: 26, r: 10, text: "text-[12px]", act: "h-[22px] px-2 text-[12px]", icon: 12 },
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
  // that is clipped (so it hides whole, never half a number); then the label truncates. The
  // action notch beside this is shrink-0 and is never overlapped.
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {lead ?? <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />}
      <span data-slot="card-state" className="flex max-h-[1lh] min-w-0 shrink flex-wrap items-center gap-x-1.5 overflow-hidden">
        <span className={cn("min-w-0 max-w-full truncate font-medium", TONE_TEXT[tone])}>{state.label}</span>
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
 * The card's inside. The surface (`--fuse`) is the top piece and, below it, the state line;
 * with an action, the state line stops short and a notch is left at the bottom-right, which
 * shows the frame behind it. The concave corner at the notch's top-left is surface colour
 * with a transparent quarter-circle, so the frame shows through the curve. The state line
 * paints 1px up into the top piece, so a fractional height never leaves a seam.
 */
function Body(props: NotchCardProps) {
  const { title, state, live, reason, meta, pr, prChecks, action, tone, notchLead, selected } = props;
  const d = DENSITY[props.density ?? "compact"];
  const done = !(state || live);
  const notched = !done && !!action;
  const r = d.r;
  const surface = "bg-[var(--fuse)] transition-colors duration-150";
  return (
    <div className="flex min-w-0 flex-col">
      <div className={cn("flex min-w-0 flex-col", d.pad, d.gap, done ? "rounded-[14.5px] pb-3" : "rounded-t-[14.5px]", surface)}>
        <span className={cn("font-medium leading-[1.4] text-strong", d.title)}>{title}</span>
        {reason && props.density !== "dense" ? (
          <span className={cn("text-[13px] leading-[1.45] text-foreground/70", props.density === "compact" && "truncate text-[12.5px]")}>{reason}</span>
        ) : null}
        {meta && (props.density === "comfortable" || done) ? (
          <span className={cn("flex min-w-0 items-center gap-2 text-muted-foreground", d.text)}>
            <span className="min-w-0 flex-1 truncate">{meta}</span>
            {pr ? <PrMark n={pr} checks={prChecks} /> : null}
          </span>
        ) : null}
      </div>
      {done ? null : (
        <span data-slot="notch-row" className={cn("flex min-w-0", d.text)} style={{ height: d.row }}>
          <span
            className={cn("relative flex min-w-0 flex-1 items-center justify-between gap-2 ps-3 pe-3 shadow-[0_-1px_0_var(--fuse)]", notched ? "rounded-bl-[14.5px]" : "rounded-b-[14.5px]", surface)}
            style={notched ? { borderBottomRightRadius: r } : undefined}
          >
            {notched ? (
              // the notch's top-left curve: surface colour with a transparent quarter-circle, the frame showing through
              <span
                aria-hidden
                className="absolute top-0 left-full shadow-[0_-1px_0_var(--fuse)] transition-colors duration-150"
                style={{ width: r, height: r, background: `radial-gradient(circle at 100% 100%, transparent ${r - 0.5}px, var(--fuse) ${r}px)` }}
              />
            ) : null}
            <span data-slot={live ? "card-live" : "card-tab"} className="flex min-w-0 items-center">
              <NotchState state={state} live={live} tone={tone} lead={notchLead} />
            </span>
            {!notched && pr ? <PrMark n={pr} checks={prChecks} /> : null}
          </span>
          {notched ? (
            <span data-notch="" className="flex shrink-0 items-center ps-1.5 pe-[3px]">
              <button
                type="button"
                data-slot="card-action"
                onClick={(e) => { e.stopPropagation(); action!.onClick(); }}
                onPointerDown={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
                className={cn(
                  "pointer-events-auto relative z-10 inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-full font-medium leading-none transition-colors duration-150",
                  "hover:bg-foreground/[0.08] active:bg-foreground/[0.12] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                  // on a selected (blue) frame the action reads in the primary's own foreground
                  selected ? "text-primary-foreground hover:bg-white/15" : TONE_TEXT[tone],
                  d.act,
                )}
              >
                <action.icon size={d.icon} weight="bold" aria-hidden />{action.label}
              </button>
            </span>
          ) : null}
        </span>
      )}
    </div>
  );
}

/**
 * The card: a thin frame wrapping the whole rounded rectangle, and, when the task waits on
 * you, the same frame flowing in to fill the notch at the bottom-right, where the action
 * sits (frame and notch are one material, like the fused tabs). The frame is a lifted grey a
 * step darker than the surface in light and lighter in dark, so it reads apart from both
 * the surface and the column. Selected, the frame turns primary; keyboard focus adds the
 * ring around the rectangle; hover brightens the frame. Nothing moves.
 */
export function NotchCard(props: NotchCardProps) {
  const { selected, ghost, lifted, handle, corner, className, style, density = "compact", ...rest } = props;
  const data = Object.fromEntries(Object.entries(rest).filter(([k]) => k.startsWith("data-")));
  return (
    <div
      {...data}
      data-density={density}
      data-selected={selected ? "" : undefined}
      style={style}
      className={cn(
        "group/card relative flex w-full flex-col rounded-[16px] p-[1.5px] text-start transition-[background-color,box-shadow] duration-150",
        "[--fuse-base:var(--card)] dark:[--fuse-base:var(--sky-pebble)] [--fuse:var(--fuse-base)]",
        "[--frame:color-mix(in_oklab,var(--foreground)_11%,var(--card))] dark:[--frame:color-mix(in_oklab,var(--foreground)_17%,var(--background))]",
        !ghost && !lifted && !selected && "hover:[--frame:color-mix(in_oklab,var(--foreground)_17%,var(--card))] dark:hover:[--frame:color-mix(in_oklab,var(--foreground)_24%,var(--background))]",
        selected && "[--frame:var(--primary)] dark:[--frame:var(--primary)] [--fuse:color-mix(in_oklab,var(--primary)_6%,var(--fuse-base))]",
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
        <Body {...props} density={density} />
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
