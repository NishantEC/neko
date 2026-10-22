// The keyboard shortcut overlay (press ?). The shell's own shortcuts are listed
// here; a page adds its own while it is open:
//
//   useEffect(() => registerShortcuts("Tasks", [{ keys: ["J"], label: "Next task" }]), []);
//
// This only DESCRIBES a shortcut. The binding is the page's own key handler.
import { useEffect, useState, useSyncExternalStore } from "react";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/ui/dialog";
import { Kbd, KbdGroup } from "@/ui/kbd";
import { MonoLabel } from "@/editorial";

export interface ShortcutRow {
  /** One key per entry: ["⌘", "K"]. */
  keys: string[];
  label: string;
}
interface Group { title: string; rows: ShortcutRow[] }

const BUILT_IN: Group[] = [
  {
    title: "Anywhere",
    rows: [
      { keys: ["⌘", "K"], label: "Open the command palette" },
      { keys: ["?"], label: "Show these shortcuts" },
      { keys: ["⌘", "N"], label: "New task (Home, cursor in the composer)" },
      { keys: ["⌘", "⇧", "Esc"], label: "Stop all work" },
      { keys: ["⌘", "B"], label: "Show or hide the sidebar" },
      { keys: ["Esc"], label: "Close a menu, dialog or the inspector" },
    ],
  },
  {
    title: "Composer",
    rows: [
      { keys: ["↵"], label: "Send, or queue while a reply runs" },
      { keys: ["⇧", "↵"], label: "New line" },
      { keys: ["⌘", "↵"], label: "Stop the reply and send this first" },
      { keys: ["/"], label: "Commands: /plan /stop /remember /memory …" },
      { keys: ["@"], label: "Mention a workspace, a tool or a file" },
      { keys: ["↑", "↓"], label: "Move in the menu; ↵ or Tab accepts, Esc closes" },
      { keys: ["⌥", "↑"], label: "Take the last queued message back to edit" },
    ],
  },
  {
    title: "A decision waiting for you",
    rows: [
      { keys: ["⌘", "↵"], label: "Approve" },
      { keys: ["Esc"], label: "Decline (when the composer is empty)" },
    ],
  },
];

let extra: Group[] = [];
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

/** Describes a page's shortcuts in the overlay; returns the function that removes them. */
export function registerShortcuts(title: string, rows: ShortcutRow[]) {
  const group = { title, rows };
  extra = [...extra, group];
  emit();
  return () => {
    extra = extra.filter((g) => g !== group);
    emit();
  };
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};
const snapshot = () => extra;

/** True when a key press is going into a text field and must not trigger a shortcut. */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

export function openShortcuts() {
  dispatchEvent(new CustomEvent("neko:shortcuts"));
}

export function ShortcutsOverlay() {
  const [open, setOpen] = useState(false);
  const pages = useSyncExternalStore(subscribe, snapshot);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === "?" || (e.key === "/" && (e.metaKey || e.ctrlKey))) && !isTyping(e.target) && !e.altKey) {
        e.preventDefault();
        setOpen((value) => !value);
      }
    };
    const onOpen = () => setOpen(true);
    addEventListener("keydown", onKey);
    addEventListener("neko:shortcuts", onOpen);
    return () => {
      removeEventListener("keydown", onKey);
      removeEventListener("neko:shortcuts", onOpen);
    };
  }, []);
  const groups = [...BUILT_IN, ...pages];
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent data-slot="shortcuts" className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Everything here works without the mouse. Press ? again to close.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-x-10 gap-y-6 sm:grid-cols-2">
          {groups.map((group) => (
            <section key={group.title} aria-label={group.title} className="flex flex-col gap-2">
              <MonoLabel className="text-[10.5px]">{group.title}</MonoLabel>
              <ul className="flex flex-col gap-1.5">
                {group.rows.map((row) => (
                  <li key={row.label} className="flex items-center justify-between gap-3 text-[13px]">
                    <span className="text-foreground">{row.label}</span>
                    <KbdGroup>{row.keys.map((key) => <Kbd key={key}>{key}</Kbd>)}</KbdGroup>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
