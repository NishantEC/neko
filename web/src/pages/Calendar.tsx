// Calendar: the person's meetings from macOS Calendar beside Neko's own dated
// work (DESIGN.md §2 and §5). The installed app reads EventKit, which already
// holds every account added in System Settings (Google, iCloud, Exchange), and
// sends the daemon 7 days back to 35 ahead every five minutes
// (native CalendarBridge.swift); this page asks the daemon for one range with
// `CalendarEvents` while it is visible. Meetings are cards with their
// calendar's colour on the left edge; Neko's work sits on the all-day row in
// Iris and opens where it lives. See docs/knowledge/calendar.md.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { request } from "@/api";
import { Button } from "@/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui/popover";
import { SegmentedControl, SegmentedControlItem } from "@/ui/segmented-control";
import { Alert, AlertDescription, AlertTitle } from "@/ui/alert";
import { ArrowSquareOut, Calendar as CalendarIcon, CaretLeft, CaretRight, Clock, MapPin, Warning } from "@/ui/icons";
import { ErrorNote, SetupPage } from "@/setup/kit";
import { relativeTime } from "@/setup/lib";
import { StatusGlyph } from "@/work/tone";
import { cn } from "@/ui/lib/utils";

const POLL_MS = 60_000;
const MINUTE = 60_000;
const HOUR_PX = 48;
/** The hours shown before scrolling: 07:00 to 20:00. */
const FIRST_HOUR = 7;
const LAST_HOUR = 20;
const ALL_DAY_SHOWN = 3;
const MONTH_SHOWN = 3;
/** A sync older than this means the app is probably not running. */
const STALE_MS = 20 * MINUTE;
const SETTINGS_URL = "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars";
const MEETING_HOSTS = ["meet.google.com", "zoom.us", "zoom.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com", "meet.jit.si", "chime.aws", "facetime.apple.com", "gotomeeting.com", "around.co"];

type View = "day" | "week" | "month";

export interface CalendarEvent {
  id: string;
  calendar: string;
  color: string;
  title: string;
  start_ms: number;
  end_ms: number;
  all_day: boolean;
  location?: string;
  url?: string;
}

export interface NekoItem {
  id: string;
  kind: "Task" | "Schedule" | "Routine";
  title: string;
  at_ms: number;
  all_day: boolean;
  status?: string;
  detail?: string;
  route: string;
}

interface Status {
  access: "NotAsked" | "Granted" | "Denied";
  source: string;
  last_sync_ms: number | null;
  events: number;
  truncated: boolean;
}

interface Data { events: CalendarEvent[]; neko: NekoItem[]; status: Status }

// ---------------------------------------------------------------------------
// Dates (local time; days are built with setDate so DST days stay whole)
// ---------------------------------------------------------------------------

export function startOfDay(d: Date): Date {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  return x;
}

export function addDays(d: Date, n: number): Date {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}

/** Monday of the week `d` is in. */
export function startOfWeek(d: Date): Date {
  const day = startOfDay(d);
  return addDays(day, -((day.getDay() + 6) % 7));
}

/** ISO 8601 week number (weeks start on Monday; week 1 holds the first Thursday). */
export function isoWeek(d: Date): number {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  t.setUTCDate(t.getUTCDate() + 4 - (t.getUTCDay() || 7));
  const yearStart = Date.UTC(t.getUTCFullYear(), 0, 1);
  return Math.ceil(((t.getTime() - yearStart) / 86_400_000 + 1) / 7);
}

export function daysFor(view: View, anchor: Date): Date[] {
  if (view === "day") return [startOfDay(anchor)];
  if (view === "week") return Array.from({ length: 7 }, (_, i) => addDays(startOfWeek(anchor), i));
  const first = startOfWeek(new Date(anchor.getFullYear(), anchor.getMonth(), 1));
  return Array.from({ length: 42 }, (_, i) => addDays(first, i));
}

function step(view: View, anchor: Date, by: number): Date {
  if (view === "day") return addDays(anchor, by);
  if (view === "week") return addDays(anchor, 7 * by);
  return new Date(anchor.getFullYear(), anchor.getMonth() + by, 1);
}

const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** Minutes since local midnight, with the day's end as 1440. */
function minuteOf(ms: number, dayEnd: number): number {
  if (ms >= dayEnd) return 24 * 60;
  const d = new Date(ms);
  return d.getHours() * 60 + d.getMinutes();
}

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const hourFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric" });
const weekdayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const longDayFormat = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
const shortDayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short" });
const monthFormat = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });
const shortMonthFormat = new Intl.DateTimeFormat(undefined, { month: "short" });
const time = (ms: number) => timeFormat.format(ms);

function titleFor(view: View, anchor: Date, days: Date[]): string {
  if (view === "day") return longDayFormat.format(anchor);
  if (view === "month") return monthFormat.format(anchor);
  const first = days[0];
  const last = days[days.length - 1];
  if (first.getMonth() === last.getMonth()) return monthFormat.format(first);
  const year = first.getFullYear() === last.getFullYear() ? ` ${last.getFullYear()}` : "";
  return year
    ? `${shortMonthFormat.format(first)} – ${shortMonthFormat.format(last)}${year}`
    : `${monthFormat.format(first)} – ${monthFormat.format(last)}`;
}

// ---------------------------------------------------------------------------
// What falls on a day, and how overlapping meetings share a column
// ---------------------------------------------------------------------------

export function overlaps(e: CalendarEvent, from: number, to: number): boolean {
  return e.start_ms < to && (e.end_ms > from || (e.end_ms === e.start_ms && e.start_ms >= from));
}

export interface Placed { event: CalendarEvent; top: number; height: number; column: number; columns: number }

/**
 * Timed events of one day as cards: each cluster of events that overlap one
 * another is split into as many side-by-side columns as it needs at its
 * busiest, and each event takes the first column free at its start.
 */
export function layoutDay(events: CalendarEvent[], day: Date): Placed[] {
  const from = day.getTime();
  const to = addDays(day, 1).getTime();
  const items = events
    .filter((e) => !e.all_day && overlaps(e, from, to))
    .map((e) => {
      const start = Math.max(e.start_ms, from);
      return { e, start, end: Math.max(Math.min(e.end_ms, to), start + 15 * MINUTE) };
    })
    .sort((a, b) => a.start - b.start || b.end - a.end || a.e.id.localeCompare(b.e.id));
  const placed: Placed[] = [];
  let cluster: Placed[] = [];
  let columnEnds: number[] = [];
  let clusterEnd = -Infinity;
  const close = () => {
    for (const p of cluster) p.columns = columnEnds.length;
    cluster = [];
    columnEnds = [];
  };
  for (const item of items) {
    if (item.start >= clusterEnd) close();
    let column = columnEnds.findIndex((end) => end <= item.start);
    if (column === -1) {
      column = columnEnds.length;
      columnEnds.push(item.end);
    } else columnEnds[column] = item.end;
    const top = (minuteOf(item.start, to) / 60) * HOUR_PX;
    const bottom = (minuteOf(item.end, to) / 60) * HOUR_PX;
    const p: Placed = { event: item.e, top, height: Math.max(bottom - top, 18), column, columns: 1 };
    cluster.push(p);
    placed.push(p);
    clusterEnd = Math.max(clusterEnd, item.end);
  }
  close();
  return placed;
}

function allDayOn(events: CalendarEvent[], day: Date): CalendarEvent[] {
  const from = day.getTime();
  const to = addDays(day, 1).getTime();
  return events.filter((e) => e.all_day && overlaps(e, from, to));
}

function nekoOn(items: NekoItem[], day: Date): NekoItem[] {
  const from = day.getTime();
  const to = addDays(day, 1).getTime();
  return items.filter((i) => i.at_ms >= from && i.at_ms < to);
}

/** Only web links open from the page. */
function webLink(url: string | undefined): string | null {
  return url && /^https?:\/\//i.test(url) ? url : null;
}

function isMeeting(url: string): boolean {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return MEETING_HOSTS.some((h) => host === h || host.endsWith(`.${h}`));
  } catch {
    return false;
  }
}

/** The calendar's colour, or mint (DESIGN.md: calendar events) when it has none. */
const tint = (e: CalendarEvent) => (/^#[0-9a-f]{6}$/i.test(e.color) ? e.color : "var(--success)");

function cardStyle(e: CalendarEvent): CSSProperties {
  const color = tint(e);
  return { borderInlineStartColor: color, background: `color-mix(in oklch, ${color} 14%, var(--card))` };
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

function useCalendar(from: number, to: number) {
  const [data, setData] = useState<Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(0);
  const load = useCallback(async () => {
    const ticket = ++latest.current;
    try {
      const reply = await request({ CalendarEvents: { from_ms: from, to_ms: to } });
      if (ticket !== latest.current) return;
      if (reply?.CalendarEvents) setData(reply.CalendarEvents as Data);
      setError(null);
    } catch (e) {
      if (ticket === latest.current) setError(e instanceof Error ? e.message : String(e));
    }
  }, [from, to]);
  useEffect(() => {
    void load();
    // A hidden page does not poll: the daemon's idle cost matters.
    const id = setInterval(() => { if (!document.hidden) void load(); }, POLL_MS);
    const onVisible = () => { if (!document.hidden) void load(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => { clearInterval(id); document.removeEventListener("visibilitychange", onVisible); };
  }, [load]);
  return { data, error, clear: () => setError(null) };
}

/** The current minute, for the now line. */
function useMinute() {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => { if (!document.hidden) setNow(Date.now()); }, 30_000);
    return () => clearInterval(id);
  }, []);
  return now;
}

const openRoute = (route: string) => { location.hash = `/${route}`; };

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

function EventDetails({ event }: { event: CalendarEvent }) {
  const link = webLink(event.url);
  const when = event.all_day
    ? `${shortDayFormat.format(event.start_ms)} · All day`
    : `${shortDayFormat.format(event.start_ms)} · ${time(event.start_ms)} – ${time(event.end_ms)}`;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <p className="text-pretty font-semibold text-foreground">{event.title || "Untitled event"}</p>
        <p className="flex items-center gap-1.5 text-[13px] tabular-nums text-muted-foreground"><Clock aria-hidden className="size-3.5 shrink-0" />{when}</p>
        {event.location ? <p className="flex items-start gap-1.5 text-[13px] text-muted-foreground"><MapPin aria-hidden className="mt-0.5 size-3.5 shrink-0" /><span className="min-w-0 break-words">{event.location}</span></p> : null}
        {event.calendar ? (
          <p className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
            <span aria-hidden className="size-2.5 shrink-0 rounded-full" style={{ background: tint(event) }} />
            {event.calendar}
          </p>
        ) : null}
      </div>
      {link ? (
        <Button size="sm" variant={isMeeting(link) ? "default" : "outline"} className="self-start" render={<a href={link} target="_blank" rel="noreferrer" />}>
          <ArrowSquareOut /> {isMeeting(link) ? "Join meeting" : "Open link"}
        </Button>
      ) : null}
    </div>
  );
}

/** An event that opens its details in a small popover. */
function EventButton({ event, className, style, children }: { event: CalendarEvent; className?: string; style?: CSSProperties; children: ReactNode }) {
  const label = event.all_day ? `${event.title || "Untitled event"}, all day` : `${event.title || "Untitled event"}, ${time(event.start_ms)} to ${time(event.end_ms)}`;
  return (
    <Popover>
      <PopoverTrigger
        render={<button type="button" aria-label={label} title={event.title} className={cn("cursor-pointer text-start outline-none focus-visible:ring-2 focus-visible:ring-ring", className)} style={style} />}
      >
        {children}
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80">
        <EventDetails event={event} />
      </PopoverContent>
    </Popover>
  );
}

function AllDayEvent({ event }: { event: CalendarEvent }) {
  return (
    <EventButton event={event} className="block w-full truncate rounded-md border-s-[3px] px-1.5 py-0.5 text-[12px] text-foreground" style={cardStyle(event)}>
      {event.title || "Untitled event"}
    </EventButton>
  );
}

function NekoChip({ item, compact = false }: { item: NekoItem; compact?: boolean }) {
  const when = item.all_day ? null : time(item.at_ms);
  const label = `${item.title}${item.detail ? `, ${item.detail}` : ""}${when ? `, ${when}` : ""}. Open in Neko.`;
  return (
    <button
      type="button"
      onClick={() => openRoute(item.route)}
      aria-label={label}
      title={[item.title, item.detail, when].filter(Boolean).join(" · ")}
      className={cn(
        "flex w-full min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-0.5 text-start text-[12px] text-primary outline-none hover:bg-primary/10 focus-visible:ring-2 focus-visible:ring-ring",
        !compact && "bg-primary/[0.06]",
      )}
    >
      {item.kind === "Task" && item.status ? <StatusGlyph status={item.status} size={12} className="shrink-0" /> : <Clock aria-hidden className="size-3 shrink-0" />}
      {when ? <span className="shrink-0 tabular-nums text-primary/70">{when}</span> : null}
      <span className="min-w-0 truncate">{item.title}</span>
    </button>
  );
}

function DayNumber({ day, today, onClick, muted = false }: { day: Date; today: boolean; onClick?: () => void; muted?: boolean }) {
  const inner = (
    <span
      className={cn(
        "inline-flex size-7 items-center justify-center rounded-full text-[13px] tabular-nums",
        today ? "bg-primary font-semibold text-primary-foreground" : muted ? "text-muted-foreground/60" : "text-foreground",
      )}
    >
      {day.getDate()}
    </span>
  );
  if (!onClick) return inner;
  return (
    <button type="button" onClick={onClick} aria-label={`${longDayFormat.format(day)}${today ? ", today" : ""}. Show this day.`} className="cursor-pointer rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring">
      {inner}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Day and week
// ---------------------------------------------------------------------------

function TimeGrid({ days, data, now, onPickDay }: { days: Date[]; data: Data; now: number; onPickDay: (d: Date) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const today = new Date(now);
  const showsToday = days.some((d) => sameDay(d, today));
  const nowMinute = minuteOf(now, addDays(startOfDay(today), 1).getTime());
  const first = days[0].getTime();
  const columns = `3.5rem repeat(${days.length}, minmax(0, 1fr))`;

  // Opens on 07:00, or on the current hour when that is later in the day.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const hour = showsToday ? Math.min(Math.max(FIRST_HOUR, Math.floor(nowMinute / 60) - 2), 24 - (LAST_HOUR - FIRST_HOUR)) : FIRST_HOUR;
    // A few pixels above the hour, so its label is not cut in half.
    el.scrollTop = hour * HOUR_PX - 8;
    // Only when the range changes, not on every minute.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [first, days.length]);

  const allDay = days.map((d) => ({ events: allDayOn(data.events, d), neko: nekoOn(data.neko, d) }));
  const tallest = Math.max(0, ...allDay.map((a) => a.events.length + a.neko.length));
  const limit = expanded || days.length === 1 ? Infinity : ALL_DAY_SHOWN;

  return (
    <div className="overflow-hidden">
      {/* Weekday header */}
      <div className="grid border-b border-border/50" style={{ gridTemplateColumns: columns }}>
        <div className="flex items-end justify-center pb-1 text-[11px] text-muted-foreground">W{isoWeek(days[0])}</div>
        {days.map((d) => {
          const isToday = sameDay(d, today);
          return (
            <div key={d.getTime()} className="flex items-center gap-1.5 border-s border-border/50 px-2 py-1.5">
              <span className={cn("text-[12px]", isToday ? "font-medium text-primary" : "text-muted-foreground")}>{weekdayFormat.format(d)}</span>
              <DayNumber day={d} today={isToday} onClick={days.length > 1 ? () => onPickDay(d) : undefined} />
            </div>
          );
        })}
      </div>
      {/* All-day row: all-day events, then Neko's dated work */}
      <div className="grid border-b border-border/50" style={{ gridTemplateColumns: columns }}>
        <div className="flex flex-col items-center justify-start gap-1 pt-1.5 text-[11px] text-muted-foreground">
          <span>All day</span>
          {tallest > ALL_DAY_SHOWN && days.length > 1 ? (
            <button type="button" onClick={() => setExpanded(!expanded)} aria-expanded={expanded} className="cursor-pointer rounded px-1 text-[11px] text-muted-foreground underline-offset-2 hover:text-foreground hover:underline">
              {expanded ? "Less" : "More"}
            </button>
          ) : null}
        </div>
        {allDay.map(({ events, neko }, i) => {
          const items = [...events.map((e) => ({ key: e.id, node: <AllDayEvent event={e} /> })), ...neko.map((n) => ({ key: n.id, node: <NekoChip item={n} /> }))];
          const hidden = items.length - Math.min(items.length, limit);
          return (
            <div key={days[i].getTime()} className="flex min-h-9 min-w-0 flex-col gap-0.5 border-s border-border/50 p-1">
              {items.slice(0, limit).map((it) => <div key={it.key} className="min-w-0">{it.node}</div>)}
              {hidden > 0 ? (
                <button type="button" onClick={() => setExpanded(true)} className="cursor-pointer rounded px-1.5 text-start text-[11px] text-muted-foreground hover:text-foreground">
                  {hidden} more
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      {/* Hours */}
      <div
        ref={scroller}
        className="relative overflow-y-auto overscroll-contain"
        style={{ height: `max(20rem, min(${(LAST_HOUR - FIRST_HOUR) * HOUR_PX}px, calc(100vh - 22rem)))` }}
      >
        <div className="relative grid" style={{ gridTemplateColumns: columns, height: 24 * HOUR_PX }}>
          <div className="relative">
            {Array.from({ length: 23 }, (_, i) => i + 1).map((h) => (
              <span key={h} className="absolute end-2 -translate-y-1/2 text-[11px] tabular-nums text-muted-foreground" style={{ top: h * HOUR_PX }}>
                {hourFormat.format(new Date(2000, 0, 1, h))}
              </span>
            ))}
          </div>
          {days.map((d) => {
            const isToday = sameDay(d, today);
            return (
              <div key={d.getTime()} className="relative border-s border-border/50">
                {Array.from({ length: 23 }, (_, i) => (
                  <div key={i} aria-hidden className="absolute inset-x-0 border-t border-border/30" style={{ top: (i + 1) * HOUR_PX }} />
                ))}
                {layoutDay(data.events, d).map((p) => (
                  <EventButton
                    key={p.event.id}
                    event={p.event}
                    className={cn(
                      "absolute flex overflow-hidden rounded-md border-s-[3px] px-1.5 text-[12px] leading-tight text-foreground hover:brightness-[1.04]",
                      p.height >= 30 ? "flex-col py-0.5" : "items-center gap-1.5 text-[11px] leading-none",
                    )}
                    style={{
                      ...cardStyle(p.event),
                      top: p.top + 1,
                      height: p.height - 2,
                      insetInlineStart: `calc(${(p.column / p.columns) * 100}% + 2px)`,
                      width: `calc(${100 / p.columns}% - 4px)`,
                    }}
                  >
                    <span className="min-w-0 truncate font-medium">{p.event.title || "Untitled event"}</span>
                    {p.height >= 34
                      ? <span className="truncate tabular-nums text-muted-foreground">{time(p.event.start_ms)} – {time(p.event.end_ms)}</span>
                      : p.height < 30 && p.columns === 1 ? <span className="shrink-0 tabular-nums text-muted-foreground">{time(p.event.start_ms)}</span> : null}
                  </EventButton>
                ))}
                {isToday ? (
                  <div aria-hidden className="pointer-events-none absolute inset-x-0 z-10" style={{ top: (nowMinute / 60) * HOUR_PX }}>
                    <div className="h-0.5 bg-primary" />
                    <div className="absolute -start-1 -top-[3px] size-2 rounded-full bg-primary" />
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Month
// ---------------------------------------------------------------------------

function MonthGrid({ days, anchor, data, now, onPickDay }: { days: Date[]; anchor: Date; data: Data; now: number; onPickDay: (d: Date) => void }) {
  const today = new Date(now);
  const weeks = Array.from({ length: days.length / 7 }, (_, i) => days.slice(i * 7, i * 7 + 7));
  const columns = "2.5rem repeat(7, minmax(0, 1fr))";
  return (
    <div className="overflow-hidden">
      <div className="grid border-b border-border/50" style={{ gridTemplateColumns: columns }}>
        <div />
        {weeks[0].map((d) => <div key={d.getTime()} className="border-s border-border/50 px-2 py-1.5 text-[12px] text-muted-foreground">{weekdayFormat.format(d)}</div>)}
      </div>
      {weeks.map((week) => (
        <div key={week[0].getTime()} className="grid border-b border-border/50 last:border-b-0" style={{ gridTemplateColumns: columns }}>
          <div className="pt-2 text-center text-[11px] tabular-nums text-muted-foreground">W{isoWeek(week[0])}</div>
          {week.map((d) => {
            const from = d.getTime();
            const to = addDays(d, 1).getTime();
            const allDay = allDayOn(data.events, d);
            const timed = data.events.filter((e) => !e.all_day && overlaps(e, from, to));
            const neko = nekoOn(data.neko, d);
            const items: { key: string; node: ReactNode }[] = [
              ...allDay.map((e) => ({ key: e.id, node: <AllDayEvent event={e} /> })),
              ...timed.map((e) => ({
                key: e.id,
                node: (
                  <EventButton event={e} className="flex w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[12px] text-foreground hover:bg-foreground/[0.05]">
                    <span aria-hidden className="size-2 shrink-0 rounded-full" style={{ background: tint(e) }} />
                    <span className="shrink-0 tabular-nums text-muted-foreground">{time(Math.max(e.start_ms, from))}</span>
                    <span className="min-w-0 truncate">{e.title || "Untitled event"}</span>
                  </EventButton>
                ),
              })),
              ...neko.map((n) => ({ key: n.id, node: <NekoChip item={n} compact /> })),
            ];
            const hidden = items.length - Math.min(items.length, MONTH_SHOWN);
            return (
              <div key={from} className="flex min-h-28 min-w-0 flex-col gap-0.5 border-s border-border/50 p-1">
                <div className="px-0.5">
                  <DayNumber day={d} today={sameDay(d, today)} muted={d.getMonth() !== anchor.getMonth()} onClick={() => onPickDay(d)} />
                </div>
                {items.slice(0, MONTH_SHOWN).map((it) => <div key={it.key} className="min-w-0">{it.node}</div>)}
                {hidden > 0 ? (
                  <button type="button" onClick={() => onPickDay(d)} className="cursor-pointer rounded px-1.5 text-start text-[11px] text-muted-foreground hover:text-foreground">
                    {hidden} more
                  </button>
                ) : null}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/** What to do when Calendar is not connected, in the person's words. */
function AccessNote({ status }: { status: Status | undefined }) {
  if (!status) return null;
  if (status.access === "Denied") {
    return (
      <Alert className="border-0 bg-foreground/[0.03]">
        <Warning aria-hidden />
        <AlertTitle>Neko can’t read your calendars</AlertTitle>
        <AlertDescription>
          <p>Allow Calendar access in System Settings → Privacy & Security → Calendars: turn on Neko, then reopen Neko. Neko’s own work still shows below.</p>
          <Button size="sm" variant="outline" className="mt-2" render={<a href={SETTINGS_URL} />}>Open Privacy & Security</Button>
        </AlertDescription>
      </Alert>
    );
  }
  if (status.access === "NotAsked" && !status.last_sync_ms) {
    return (
      <p
        data-slot="calendar-not-asked"
        title="Neko reads macOS Calendar, which already includes the Google, iCloud and Exchange accounts added in System Settings → Internet Accounts. If you chose Don’t Allow before, allow it in System Settings → Privacy & Security → Calendars."
        className="flex items-center gap-2 text-[13px] text-muted-foreground"
      >
        <CalendarIcon aria-hidden size={14} className="shrink-0" />
        <span><span className="text-foreground">Your meetings are not here yet.</span> Open the Neko app and choose Allow when macOS asks about Calendar.</span>
      </p>
    );
  }
  return null;
}

/** Where the events came from and when, for the Today button's tooltip. */
function syncText(status: Status | undefined, now: number): string | undefined {
  if (!status || status.access !== "Granted") return undefined;
  return `From ${status.source || "macOS Calendar"}, synced ${relativeTime(status.last_sync_ms, now)}.`;
}

/**
 * Said aloud always, shown only when it matters (DESIGN.md §8): an empty
 * span, a stale sync or a truncated list. The plain "synced 1 minute ago" is
 * the Today button's tooltip.
 */
function SyncLine({ status, now, empty, view }: { status: Status | undefined; now: number; empty: boolean; view: View }) {
  if (!status || status.access !== "Granted") return null;
  const stale = status.last_sync_ms != null && now - status.last_sync_ms > STALE_MS;
  const span = view === "day" ? "today" : view === "week" ? "this week" : "this month";
  const visible = empty || stale || status.truncated;
  return (
    <p role="status" aria-live="polite" className={visible ? "text-[12.5px] text-muted-foreground" : "sr-only"}>
      {empty ? `No meetings ${span}. ` : null}
      From {status.source || "macOS Calendar"}, synced {relativeTime(status.last_sync_ms, now)}
      {stale ? ". Neko may not be running; events update while the app is open." : "."}
      {status.truncated ? " Showing the first 2000 events." : null}
    </p>
  );
}

export default function CalendarPage() {
  const [view, setView] = useState<View>("week");
  const [anchor, setAnchor] = useState(() => startOfDay(new Date()));
  const now = useMinute();
  const days = useMemo(() => daysFor(view, anchor), [view, anchor]);
  const from = days[0].getTime();
  const to = addDays(days[days.length - 1], 1).getTime();
  const { data, error, clear } = useCalendar(from, to);
  const pickDay = (d: Date) => { setAnchor(startOfDay(d)); setView("day"); };
  const today = new Date(now);
  const showsToday = days.some((d) => sameDay(d, today)) && (view !== "month" || anchor.getMonth() === today.getMonth());
  const week = isoWeek(days[0]);

  const actions = (
    <>
      <Button variant="ghost" size="sm" onClick={() => setAnchor(startOfDay(new Date()))} disabled={showsToday} title={syncText(data?.status, now)}>Today</Button>
      <div className="flex items-center">
        <Button variant="ghost" size="icon-sm" aria-label={`Previous ${view}`} onClick={() => setAnchor((a) => step(view, a, -1))}><CaretLeft /></Button>
        <Button variant="ghost" size="icon-sm" aria-label={`Next ${view}`} onClick={() => setAnchor((a) => step(view, a, 1))}><CaretRight /></Button>
      </div>
      <SegmentedControl value={view} onValueChange={(v) => setView(v as View)} aria-label="Calendar view">
        <SegmentedControlItem value="day">Day</SegmentedControlItem>
        <SegmentedControlItem value="week">Week</SegmentedControlItem>
        <SegmentedControlItem value="month">Month</SegmentedControlItem>
      </SegmentedControl>
    </>
  );

  const title = <span title={syncText(data?.status, now)}>{titleFor(view, anchor, days)}</span>;

  const empty = !!data && data.events.length === 0;
  return (
    <SetupPage title={title} count={view !== "month" ? `Week ${week}` : undefined} actions={actions} width="max-w-[1400px]">
      <div className="flex flex-col gap-3">
        <ErrorNote error={error} title="Can’t reach the Neko daemon" onDismiss={clear} />
        <AccessNote status={data?.status} />
        <SyncLine status={data?.status} now={now} empty={empty} view={view} />
        {data ? (
          view === "month"
            ? <MonthGrid days={days} anchor={anchor} data={data} now={now} onPickDay={pickDay} />
            : <TimeGrid days={days} data={data} now={now} onPickDay={pickDay} />
        ) : error ? null : (
          <div aria-busy="true" aria-label="Loading the calendar" className="h-96 animate-pulse rounded-xl bg-foreground/[0.03]" />
        )}
      </div>
    </SetupPage>
  );
}
