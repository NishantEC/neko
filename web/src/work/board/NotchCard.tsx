// The task card: one solid surface with a notch carved from its bottom-left edge, the
// column showing through it (the owner's "ticket with a bite taken out"). The state sits
// in the notch, off the card; the action, when the task waits on you, sits on the card to
// the right. Done cards have no notch. Drawn once here for the board (BoardCard) and the
// gallery's specimens, so the two can never drift.
//
// How it is built: two copies of one layout share a grid cell. The back copy paints the
// pieces (the top piece, and beside the notch a bottom piece with the concave corner) with
// its text invisible, and carries the filter: in light a shadow that follows the shape,
// notch included; selected or focused, an outline drawn with drop-shadows (a border or
// ring on each piece would draw seams; a filter on the text would fringe every glyph). The
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
  /** What the notch says. Null: no notch (a done card). */
  state: { label: string; age?: string; detail?: string } | null;
  /** Running: the step and its clock replace the state, the step shimmering. */
  live?: { step: string; clock: string } | null;
  reason?: string | null;
  /** The well's meta line (comfortable), and a done card's last line at every density. */
  meta?: ReactNode;
  /** A pull request number, shown on the card beside the notch when there is no action. */
  pr?: string | number | null;
  /** Checks on that pull request, as a dot on its mark. */
  prChecks?: "passing" | "failing" | null;
  action?: NotchAction | null;
  density?: Density;
  /** Leads the notch: a source glyph, an agent's mark. */
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

function Body(props: NotchCardProps & { surface?: boolean }) {
  const { title, state, live, reason, meta, pr, prChecks, action, tone, notchLead, surface } = props;
  const d = DENSITY[props.density ?? "compact"];
  const notched = !!(state || live);
  const paint = surface ? "bg-[var(--fuse)] transition-colors duration-150 [&>*:not([data-corner])]:invisible" : "";
  const r = d.r;
  return (
    <div className="flex flex-col">
      <div
        className={cn("flex flex-col", d.pad, d.gap, notched ? "rounded-t-[16px]" : "rounded-[16px]", !notched && "pb-3", paint)}
        style={notched ? { borderBottomLeftRadius: r } : undefined}
      >
        <span className={cn("font-medium leading-[1.4] text-strong", d.title)}>{title}</span>
        {reason && props.density !== "dense" ? (
          <span className={cn("text-[13px] leading-[1.45] text-foreground/70", props.density === "compact" && "truncate text-[12.5px]")}>{reason}</span>
        ) : null}
        {meta && (props.density === "comfortable" || !notched) ? (
          <span className={cn("flex min-w-0 items-center gap-2 text-muted-foreground", d.text)}>
            <span className="min-w-0 flex-1 truncate">{meta}</span>
            {pr ? <PrMark n={pr} checks={prChecks} /> : null}
          </span>
        ) : null}
      </div>
      {notched ? (
        <span data-slot={surface ? undefined : "notch-row"} className={cn("flex", d.text)} style={{ height: d.row }}>
          <span
            data-notch={surface ? undefined : ""}
            data-slot={surface ? undefined : live ? "card-live" : "card-tab"}
            className="flex min-w-0 max-w-[68%] shrink-0 items-center ps-3 pe-2.5"
          >
            <NotchState state={state} live={live} tone={tone} lead={notchLead} />
          </span>
          <span
            className={cn("relative flex min-w-0 flex-1 items-center justify-end rounded-b-[16px] pe-1.5", surface && "shadow-[0_-1px_0_var(--fuse)]", paint)}
            style={{ borderBottomLeftRadius: r }}
          >
            {surface ? (
              // the notch's top-right corner: card colour with a transparent quarter-circle
              <span
                data-corner
                className="absolute top-0 right-full shadow-[0_-1px_0_var(--fuse)]"
                style={{ width: r, height: r, background: `radial-gradient(circle at 0% 100%, transparent ${r - 0.5}px, var(--fuse) ${r}px)` }}
              />
            ) : action ? (
              <button
                type="button"
                data-slot="card-action"
                onClick={(e) => { e.stopPropagation(); action.onClick(); }}
                onPointerDown={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
                className={cn(
                  "pointer-events-auto relative z-10 inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-full font-medium leading-none text-foreground/85 transition-colors duration-150",
                  "hover:bg-foreground/[0.07] hover:text-foreground active:bg-foreground/[0.1] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                  d.act,
                )}
              >
                <action.icon size={d.icon} weight="bold" aria-hidden />{action.label}
              </button>
            ) : pr ? <span className="pe-1.5"><PrMark n={pr} checks={prChecks} /></span> : null}
          </span>
        </span>
      ) : null}
    </div>
  );
}

const OUTLINE = (c: string, w = 1) =>
  `[filter:drop-shadow(${w}px_0_0_${c})_drop-shadow(-${w}px_0_0_${c})_drop-shadow(0_${w}px_0_${c})_drop-shadow(0_-${w}px_0_${c})]`;

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
        "[--fuse:var(--fuse-base)]",
        !ghost && !lifted && "hover:[--fuse:color-mix(in_oklab,var(--foreground)_3%,var(--fuse-base))] transition-transform duration-[400ms] ease-[cubic-bezier(0.22,1,0.36,1)] hover:-translate-y-[2px] motion-reduce:transition-none motion-reduce:hover:translate-y-0",
        selected && "[--fuse:color-mix(in_oklab,var(--primary)_7%,var(--fuse-base))] hover:[--fuse:color-mix(in_oklab,var(--primary)_10%,var(--fuse-base))]",
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
            lifted
              ? "[filter:drop-shadow(0_0_0.5px_rgb(0_0_0/0.2))_drop-shadow(0_10px_20px_rgb(0_0_0/0.22))]"
              : selected
                ? OUTLINE("var(--primary)")
                : "[filter:drop-shadow(0_0_0.5px_rgb(0_0_0/0.14))_drop-shadow(0_1px_2px_rgb(0_0_0/0.06))] dark:[filter:none]",
            // keyboard focus anywhere in the card outlines the whole shape in the ring colour
            "group-has-[:focus-visible]/card:[filter:drop-shadow(1.5px_0_0_var(--ring))_drop-shadow(-1.5px_0_0_var(--ring))_drop-shadow(0_1.5px_0_var(--ring))_drop-shadow(0_-1.5px_0_var(--ring))]",
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
