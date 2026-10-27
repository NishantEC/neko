// The task card: one solid surface whose last line is the state. When the task waits on
// you, a notch is carved from its bottom-right edge, the column showing through it (the
// owner's "ticket with a bite taken out"), and the action sits in it in the state's
// colour. No action, no notch; done cards end on their meta line. Drawn once here for the board (BoardCard) and the
// gallery's specimens, so the two can never drift.
//
// How it is built: two copies of one layout share a grid cell. The back copy paints the
// pieces (the top piece, and the state line beside the notch with the concave corner) with
// its text invisible, and carries the filter: a 1px border (four drop-shadows) that follows
// the shape, notch included, grey at rest, primary when selected, the ring colour on focus
// (a border on each piece would draw seams; a filter on the text would fringe every glyph). The
// front copy carries the text and the action. The bottom piece paints 1px up into the top
// one, so a fractional height never leaves a hairline seam. One `--fuse` colour paints
// every piece, so hover and selection change them together.
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
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {lead ?? <span aria-hidden className={cn("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />}
      <span className={cn("shrink-0 font-medium", TONE_TEXT[tone])}>{state.label}</span>
      {state.age ? <span className="shrink-0 tabular-nums text-muted-foreground">{state.age}</span> : null}
      {state.detail ? <span className="min-w-0 truncate text-muted-foreground">· {state.detail}</span> : null}
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

/** The action's inert twin in the surface copy: the same size as the button. */
function SurfaceAction({ action, className, size }: { action: NotchAction; className: string; size: number }) {
  const Icon = action.icon;
  return <span className={cn("inline-flex items-center gap-1.5 font-medium", className)}><Icon size={size} weight="bold" />{action.label}</span>;
}

function Body(props: NotchCardProps & { surface?: boolean }) {
  const { title, state, live, reason, meta, pr, prChecks, action, tone, notchLead, surface } = props;
  const d = DENSITY[props.density ?? "compact"];
  const done = !(state || live);
  const notched = !done && !!action;
  const paint = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  const r = d.r;
  return (
    <div className="flex flex-col">
      <div className={cn("flex flex-col", d.pad, d.gap, done ? "rounded-[16px] pb-3" : "rounded-t-[16px]", paint)}>
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
        // The state is the card's last line; with an action, a notch is carved bottom-right to hold it.
        <span data-slot={surface ? undefined : "notch-row"} className={cn("flex", d.text)} style={{ height: d.row }}>
          <span
            className={cn("relative flex min-w-0 flex-1 items-center justify-between gap-2 ps-3 pe-3", notched ? "rounded-bl-[16px]" : "rounded-b-[16px]", surface && "shadow-[0_-1px_0_var(--fuse)]", paint)}
            style={notched ? { borderBottomRightRadius: r } : undefined}
          >
            {surface && notched ? (
              // the notch's top-left corner: card colour with a transparent quarter-circle
              <span
                data-corner
                className="absolute top-0 left-full shadow-[0_-1px_0_var(--fuse)]"
                style={{ width: r, height: r, background: `radial-gradient(circle at 100% 100%, transparent ${r - 0.5}px, var(--fuse) ${r}px)` }}
              />
            ) : null}
            <span data-slot={surface ? undefined : live ? "card-live" : "card-tab"} className="flex min-w-0 items-center">
              <NotchState state={state} live={live} tone={tone} lead={notchLead} />
            </span>
            {!notched && pr ? <PrMark n={pr} checks={prChecks} /> : null}
          </span>
          {notched ? (
            <span data-notch={surface ? undefined : ""} className="flex shrink-0 items-center ps-1.5 pe-0.5">
              {surface ? (
                // the same size as the button, so both copies lay out alike
                <SurfaceAction action={action!} className={d.act} size={d.icon} />
              ) : (
                <button
                  type="button"
                  data-slot="card-action"
                  onClick={(e) => { e.stopPropagation(); action!.onClick(); }}
                  onPointerDown={(e) => e.stopPropagation()}
                  onKeyDown={(e) => e.stopPropagation()}
                  className={cn(
                    "pointer-events-auto relative z-10 inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-full font-medium leading-none transition-colors duration-150",
                    "hover:bg-foreground/[0.07] active:bg-foreground/[0.1] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    TONE_TEXT[tone], d.act,
                  )}
                >
                  <action.icon size={d.icon} weight="bold" aria-hidden />{action.label}
                </button>
              )}
            </span>
          ) : null}
        </span>
      )}
    </div>
  );
}

// The border is four 1px drop-shadows in `--edge`, so it follows the whole notched shape,
// concave curves included, and the two pieces never draw a line where they meet. Written
// out in full: Tailwind only generates classes it can read in the source.
const EDGE = "[filter:drop-shadow(1px_0_0_var(--edge))_drop-shadow(-1px_0_0_var(--edge))_drop-shadow(0_1px_0_var(--edge))_drop-shadow(0_-1px_0_var(--edge))]";

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
        "group/card relative flex w-full flex-col text-start",
        "[--fuse-base:var(--card)] dark:[--fuse-base:var(--sky-pebble)]",
        // the border is Neko's --border grey; hover lightens the surface and darkens the border, nothing moves
        "[--fuse:var(--fuse-base)] [--edge:var(--border)]",
        !ghost && !lifted && "hover:[--fuse:color-mix(in_oklab,var(--foreground)_3%,var(--fuse-base))] hover:[--edge:color-mix(in_oklab,var(--foreground)_22%,var(--border))]",
        // selected: the primary colour replaces the grey border, and the surface takes a faint tint
        selected && "[--edge:var(--primary)] hover:[--edge:var(--primary)] [--fuse:color-mix(in_oklab,var(--primary)_7%,var(--fuse-base))] hover:[--fuse:color-mix(in_oklab,var(--primary)_10%,var(--fuse-base))]",
        // keyboard focus anywhere in the card: the ring colour
        "has-[:focus-visible]:[--edge:var(--ring)]",
        ghost && "opacity-40",
        className,
      )}
    >
      {handle}
      <div className="pointer-events-none grid min-w-0 grid-cols-[minmax(0,1fr)]">
        <div
          aria-hidden
          className={cn(
            "[grid-area:1/1] transition-[filter] duration-150",
            lifted ? "[filter:drop-shadow(0_0_0.5px_rgb(0_0_0/0.2))_drop-shadow(0_10px_20px_rgb(0_0_0/0.22))]" : EDGE,
          )}
        >
          <Body {...props} density={density} surface />
        </div>
        {/* positioned, so it paints above the filtered surface; the action re-enables its own pointer events */}
        <div className="relative [grid-area:1/1]">
          <Body {...props} density={density} />
        </div>
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
