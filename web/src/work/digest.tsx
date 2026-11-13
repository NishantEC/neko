// The morning digest on Home (docs/knowledge/morning-digest.md, DESIGN.md §6): one calm
// block above the conversation. Status first, then the bottleneck, then what needs you
// ("Nothing needs you right now" when true). Details open on click; "Mark read" hides it
// (completing its reminder on the phone does the same). Nothing when there is no digest.
import { useCallback, useEffect, useState } from "react";
import { request, type Json } from "@/api";
import { CaretDown, Check } from "@/ui/icons";
import { RowAction, RowActions } from "@/setup/kit";
import { cn } from "@/ui/lib/utils";

const POLL_MS = 60_000;

interface Line { text: string; default: string | null }
interface Digest {
  at_ms: number;
  day: string;
  headline: string;
  finished: Line[];
  stuck: Line[];
  decisions: Line[];
  calendar: string;
  spend: string;
  read: boolean;
}

function Group({ title, lines, slot, showDefault }: { title: string; lines: Line[]; slot: string; showDefault?: boolean }) {
  if (!lines.length) return null;
  return (
    <div data-slot={slot} className="flex flex-col gap-0.5">
      <p className="text-[12.5px] font-medium text-muted-foreground">{title}</p>
      <ul className="flex flex-col gap-0.5">
        {lines.map((line, index) => (
          <li key={index} className="max-w-[80ch] text-[13px] text-foreground">
            {line.text}
            {showDefault || line.default ? <span className="text-muted-foreground"> · {line.default ? `default: ${line.default}` : "No default"}</span> : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function HomeDigest() {
  const [digest, setDigest] = useState<Digest | null>(null);
  const [open, setOpen] = useState(false);
  const take = useCallback((reply: Json) => {
    if (reply && "Digest" in reply) setDigest((reply.Digest.digest as Digest | null) ?? null);
  }, []);
  useEffect(() => {
    let alive = true;
    const load = () => request("MorningDigest").then((r) => { if (alive) take(r); }, () => {});
    void load();
    const id = setInterval(() => { if (!document.hidden) void load(); }, POLL_MS);
    return () => { alive = false; clearInterval(id); };
  }, [take]);
  if (!digest) return null;
  const markRead = () => void request("MarkDigestRead").then(take, () => {});
  return (
    <section data-slot="home-digest" aria-label="Morning digest" className="mx-auto flex w-full max-w-(--chat-content-max-width) shrink-0 flex-col px-3 pt-1">
      <div className="group/row flex min-h-8 items-start gap-3 rounded-[10px] px-3 py-1.5 transition-colors hover:bg-foreground/[0.03]">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex min-w-0 flex-1 items-start gap-2 rounded-md text-start outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <CaretDown aria-hidden size={12} className={cn("mt-1 shrink-0 text-muted-foreground transition-transform", !open && "-rotate-90")} />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[12.5px] text-muted-foreground">Morning digest</span>
            <span data-slot="digest-headline" className="max-w-[80ch] text-[13.5px] text-foreground">{digest.headline}</span>
          </span>
        </button>
        <RowActions className="hidden group-hover/row:flex group-focus-within/row:flex [@media(hover:none)]:flex">
          <RowAction label="Mark the digest read" icon={Check} onClick={markRead} />
        </RowActions>
      </div>
      {open ? (
        <div data-slot="digest-details" className="flex flex-col gap-3 ps-11 pe-3 pb-2 pt-1">
          <Group title="Finished" lines={digest.finished} slot="digest-finished" />
          <Group title="Stuck" lines={digest.stuck} slot="digest-stuck" />
          <Group title="Needs a decision" lines={digest.decisions} slot="digest-decisions" showDefault />
          {digest.calendar ? <p data-slot="digest-calendar" className="text-[13px] text-muted-foreground">{digest.calendar}</p> : null}
          {digest.spend ? <p data-slot="digest-spend" className="text-[13px] text-muted-foreground">{digest.spend}</p> : null}
        </div>
      ) : null}
    </section>
  );
}
