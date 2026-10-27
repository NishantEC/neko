// Static sample data for the gallery's board options (/#/ui, "Board"). Shaped
// like the owner's real board on a normal day (Needs you 45, Ready to review 8,
// Running 0, Up next 0, Done 81); never live.
import type { Stage } from "@/work/model";

/** Why a Needs-you task is waiting: the one state that differs from its column. */
export type Waiting = "stopped" | "asks" | "approve";

export type SampleCard = {
  id: string;
  /** Neko's short id, e.g. NEK-5AFF. */
  key: string;
  title: string;
  repo: string;
  /** Where it came from: "Sentry", or a Linear key such as "HEA-3004". */
  source?: string;
  /** Minutes ago it last changed. */
  age: number;
  pr?: number;
  stage: Stage;
  waiting?: Waiting;
  /** One sentence on why it needs you (Needs you only). */
  reason?: string;
};

export const WAITING_LABEL: Record<Waiting, string> = { stopped: "Stopped", asks: "Asks you", approve: "Approve" };
export const WAITING_ACTION: Record<Waiting, string> = { stopped: "Start again", asks: "Answer", approve: "Approve" };

type Template = Omit<SampleCard, "id" | "key" | "age" | "stage">;

const NEEDS_YOU: Template[] = [
  { title: "Fix TypeError in meal logging when portion is null", repo: "athena", source: "Sentry", waiting: "stopped", reason: "Actual changed files exceed verified or approved scope." },
  { title: "HEA-3004: Coach chat drops messages after reconnect", repo: "athena", source: "HEA-3004", waiting: "asks", reason: "Should reconnects replay unread messages or only the last one?" },
  { title: "Bump expo-notifications to 0.29 and fix iOS badge count", repo: "navihealth", source: "HEA-2987", waiting: "approve", reason: "Plan touches 14 files across two packages.", pr: 4649 },
  { title: "Rate-limit the /v2/insights endpoint per user", repo: "athena", source: "Sentry", waiting: "stopped", reason: "Tests failed three times on the same assertion." },
  { title: "Daily summary email shows yesterday’s steps twice", repo: "athena", source: "HEA-3011", waiting: "asks", reason: "Which timezone should the summary use for travellers?" },
  { title: "Remove the legacy onboarding flag from settings", repo: "navihealth", waiting: "approve", reason: "Deletes a migration; needs your go-ahead." },
  { title: "KeyError: 'goal_id' in weekly report worker", repo: "athena", source: "Sentry", waiting: "stopped", reason: "Run timed out after 30 minutes." },
  { title: "Water tracker resets at midnight UTC instead of local", repo: "navihealth", source: "HEA-2990", waiting: "stopped", reason: "Branch has conflicts with master." },
  { title: "Add retry to the Stripe webhook handler", repo: "athena", source: "HEA-3019", waiting: "approve", reason: "Plan adds a new queue; approve before it builds." },
  { title: "Profile photo upload fails above 4 MB on Android", repo: "navihealth", source: "Sentry", waiting: "asks", reason: "Resize on the device, or raise the server limit?" },
];

const REVIEW: Template[] = [
  { title: "Cache food search results for 10 minutes", repo: "athena", source: "HEA-2975", pr: 4641 },
  { title: "Show the coach’s typing indicator in chat", repo: "navihealth", source: "HEA-2968", pr: 4638 },
  { title: "Fix N+1 query in the meal plan list", repo: "athena", source: "Sentry", pr: 4652 },
  { title: "Dark mode colours for the progress ring", repo: "navihealth", pr: 4633 },
];

const DONE: Template[] = [
  { title: "Update athena #4641 branch onto master", repo: "athena", pr: 4641 },
  { title: "ValueError in recipe import for empty ingredients", repo: "athena", source: "Sentry", pr: 4620 },
  { title: "HEA-2950: Allow editing a logged meal’s time", repo: "navihealth", source: "HEA-2950", pr: 4611 },
  { title: "Drop the unused analytics SDK", repo: "navihealth", pr: 4602 },
  { title: "Log slow queries over 500 ms", repo: "athena" },
  { title: "Retry push token registration on 503", repo: "navihealth", source: "Sentry", pr: 4598 },
];

function hex(n: number) {
  return ((n * 2654435761) >>> 0).toString(16).toUpperCase().slice(-4).padStart(4, "0");
}

function fill(stage: Stage, from: Template[], count: number, seed: number, startAge: number): SampleCard[] {
  return Array.from({ length: count }, (_, i) => {
    const t = from[i % from.length];
    return { ...t, id: `${stage}-${i}`, key: `NEK-${hex(seed + i)}`, age: startAge + i * 47 + (i % 3) * 13, stage };
  });
}

/** A fresh copy of the sample board (each option moves its own cards). */
export function sampleBoard(): SampleCard[] {
  return [
    ...fill("needsYou", NEEDS_YOU, 45, 11, 18),
    ...fill("review", REVIEW, 8, 401, 35),
    ...fill("done", DONE, 81, 907, 120),
  ];
}

/** "6h", "3d": the age as short as it reads. */
export function shortAge(minutes: number): string {
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 60 * 24) return `${Math.round(minutes / 60)}h`;
  return `${Math.round(minutes / (60 * 24))}d`;
}
