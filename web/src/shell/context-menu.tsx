// Neko's own right-click menu, everywhere, instead of the browser's. One host
// (AppShell) owns the menu; a surface only says which items it offers:
//
//   function TaskRow({ task }) {
//     const menu = useContextMenu(() => [
//       { label: "Open in a tab", icon: ArrowSquareOut, run: () => navigate(…) },
//       "separator",
//       { label: "Delete…", destructive: true, run: () => … },
//     ]);
//     return <button {...menu} …>…</button>;   // spreads `data-context-menu`
//   }
//
//   <WithContextMenu items={() => […]}>…</WithContextMenu>   // without a hook (display: contents)
//
// Right-click (or Shift+F10 / the context-menu key on the focused element)
// finds the nearest element that offers items. Selected text adds Copy, Ask
// Neko about this and Search on top. Where nothing offers items the small
// default menu shows (back, forward, reload, new tab, ⌘K). Inside a text field
// or contenteditable the browser's own menu stays (spelling, paste) unless the
// field itself offers items. Items run the same functions as the visible
// buttons; this file adds no daemon requests.
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { ContextMenu as Primitive } from "@base-ui/react/context-menu";
import {
  ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuShortcut,
} from "@/ui/context-menu";
import { ArrowClockwise, ArrowLeft, ArrowRight, ChatCircle, Copy, MagnifyingGlass, Plus, Command as CommandIcon, type Icon } from "@/ui/icons";
import { toast } from "@/ui/sonner";

export interface MenuItem {
  label: string;
  icon?: Icon;
  /** A hint on the right ("⌘W"); it does not bind the key. */
  shortcut?: string;
  disabled?: boolean;
  destructive?: boolean;
  run: () => void;
}
export type MenuEntry = MenuItem | "separator" | null | false | undefined;
export type MenuItems = () => MenuEntry[];

const ATTR = "data-context-menu";
const providers = new Map<string, MenuItems>();

/** Registers the items a surface offers; spread the result on its root element. */
export function useContextMenu(items: MenuItems): { [ATTR]: string } {
  const id = useId();
  const latest = useRef(items);
  latest.current = items;
  useEffect(() => {
    providers.set(id, () => latest.current());
    return () => void providers.delete(id);
  }, [id]);
  return { [ATTR]: id };
}

/** The same, as a wrapper that adds no box (display: contents). */
export function WithContextMenu({ items, children }: { items: MenuItems; children: ReactNode }) {
  const props = useContextMenu(items);
  return <span {...props} className="contents">{children}</span>;
}

// ---------------------------------------------------------------------------
// What the shell hands the default and selection menus (set by AppShell, so
// this file does not import the tabs or the palette).
// ---------------------------------------------------------------------------

export interface ShellMenuActions {
  back(): void;
  forward(): void;
  canGoBack(): boolean;
  canGoForward(): boolean;
  newTab(): void;
  openPalette(): void;
  /** Home's composer, prefilled with a quote. */
  askNeko(text: string): void;
  /** ⌘K with the text as the search. */
  search(text: string): void;
}
let shell: ShellMenuActions | null = null;
export function setShellMenuActions(actions: ShellMenuActions) {
  shell = actions;
}

export async function copyText(text: string, what = "Copied") {
  try {
    await navigator.clipboard.writeText(text);
    toast(what);
  } catch {
    toast.error("Could not copy");
  }
}

const isField = (el: Element | null) =>
  !!el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || (el as HTMLElement).isContentEditable || !!el.closest("[contenteditable=''],[contenteditable='true']"));

function selectionAt(target: Element): string {
  const selection = getSelection();
  const text = selection?.toString().trim() ?? "";
  if (!text || !selection || selection.rangeCount === 0) return "";
  // Only when the click is on (or inside) the selected text.
  return selection.getRangeAt(0).intersectsNode(target) ? text : "";
}

function defaultItems(): MenuEntry[] {
  return [
    { label: "Back", icon: ArrowLeft, shortcut: "⌘[", disabled: !shell?.canGoBack(), run: () => shell?.back() },
    { label: "Forward", icon: ArrowRight, shortcut: "⌘]", disabled: !shell?.canGoForward(), run: () => shell?.forward() },
    { label: "Reload page", icon: ArrowClockwise, run: () => location.reload() },
    "separator",
    { label: "New tab", icon: Plus, shortcut: "⌘T", run: () => shell?.newTab() },
    { label: "Open the command palette", icon: CommandIcon, shortcut: "⌘K", run: () => shell?.openPalette() },
  ];
}

function selectionItems(text: string): MenuEntry[] {
  const short = text.length > 24 ? `${text.slice(0, 24)}…` : text;
  return [
    { label: "Copy", icon: Copy, shortcut: "⌘C", run: () => void copyText(text) },
    { label: "Ask Neko about this", icon: ChatCircle, run: () => shell?.askNeko(text) },
    { label: `Search for “${short}”`, icon: MagnifyingGlass, run: () => shell?.search(text) },
  ];
}

/** What the menu shows for a right-click on `target`, or null for the browser's own menu. */
export function itemsFor(target: Element): MenuEntry[] | null {
  const owner = target.closest(`[${ATTR}]`);
  const field = isField(target);
  // A text field keeps the browser's menu, unless the field itself (or something inside it) offers items.
  if (field && !(owner && isField(owner))) return null;
  const text = field ? "" : selectionAt(target);
  const own = owner ? providers.get(owner.getAttribute(ATTR) ?? "")?.() ?? [] : [];
  const items = own.filter(Boolean);
  if (text) return items.length ? [...selectionItems(text), "separator", ...items] : selectionItems(text);
  return items.length ? items : defaultItems();
}

/** Drops leading, trailing and doubled separators, and an item whose words already appeared (two sources offering the same action). */
function tidy(entries: MenuEntry[]): (MenuItem | "separator")[] {
  const out: (MenuItem | "separator")[] = [];
  const seen = new Set<string>();
  for (const e of entries) {
    if (!e) continue;
    if (e !== "separator") {
      if (seen.has(e.label)) continue;
      seen.add(e.label);
    }
    if (e === "separator" && (out.length === 0 || out[out.length - 1] === "separator")) continue;
    out.push(e);
  }
  while (out[out.length - 1] === "separator") out.pop();
  return out;
}

/**
 * The one menu. Wraps the whole app: a right-click anywhere opens it at the
 * pointer with the items of what was clicked.
 */
export function ContextMenuHost({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<(MenuItem | "separator")[]>([]);
  const returnTo = useRef<HTMLElement | null>(null);

  // Shift+F10 and the context-menu key open the menu on the focused element.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu")) return;
      const el = document.activeElement;
      if (!(el instanceof HTMLElement) || el === document.body) return;
      if (isField(el) && !el.closest(`[${ATTR}]`)) return;
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: rect.left + Math.min(16, rect.width / 2), clientY: rect.top + Math.min(rect.height, 28) }));
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      className="contents"
      onContextMenuCapture={(event) => {
        const target = event.target instanceof Element ? event.target : null;
        const next = target ? itemsFor(target) : null;
        // Null: the browser's own menu (a text field). Stop it reaching the trigger, which would prevent it.
        if (!next) {
          event.stopPropagation();
          return;
        }
        const list = tidy(next);
        if (list.length === 0) {
          event.stopPropagation();
          event.preventDefault();
          return;
        }
        returnTo.current = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
        setItems(list);
      }}
    >
      <ContextMenu>
        <Primitive.Trigger render={<div data-slot="context-menu-area" className="contents" />}>{children}</Primitive.Trigger>
        <ContextMenuContent data-slot="neko-context-menu" aria-label="Actions" className="min-w-52" finalFocus={() => returnTo.current ?? true}>
          {items.map((item, index) =>
            item === "separator" ? (
              <ContextMenuSeparator key={`s${index}`} />
            ) : (
              <ContextMenuItem
                key={`${index}:${item.label}`}
                disabled={item.disabled}
                variant={item.destructive ? "destructive" : "default"}
                onClick={() => {
                  // After the menu has closed, so a dialog it opens gets the focus.
                  setTimeout(item.run, 0);
                }}
              >
                {item.icon ? <item.icon aria-hidden /> : <span aria-hidden className="size-4" />}
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.shortcut ? <ContextMenuShortcut>{item.shortcut}</ContextMenuShortcut> : null}
              </ContextMenuItem>
            ),
          )}
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}
