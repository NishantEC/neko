// The inspector: a resizable panel on the right of the page that any page can
// open (the web twin of SidePanel.swift). Drag the handle, or focus it and use
// the arrow keys; the width is remembered. Esc closes it from inside.
//
// Two ways to use it from a page:
//
//   // 1. Declarative: the content is part of YOUR component, so it stays live
//   //    with your state. Closing is your state change.
//   const [selected, setSelected] = useState<Task | null>(null);
//   return (
//     <>
//       <TaskList onSelect={setSelected} />
//       <Inspector open={!!selected} onClose={() => setSelected(null)}
//                  title={selected?.title} subtitle="Task">
//         <TaskDetail task={selected} />
//       </Inspector>
//     </>
//   );
//
//   // 2. Imperative, for one-off content:
//   const inspector = useInspector();
//   inspector.open({ title: "Command", children: <pre>{text}</pre> });
//   inspector.close();
//
// Only one inspector is open at a time; the last one opened wins. The host
// (AppShell) renders it, so pages never lay out the panel themselves.
import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/ui/resizable";
import { Button } from "@/ui/button";
import { X } from "@/ui/icons";
import { MonoLabel } from "@/editorial";

interface Meta {
  title?: ReactNode;
  subtitle?: ReactNode;
  onClose?: () => void;
}
interface State { owner: string; meta: Meta; node?: ReactNode }

interface Api {
  state: State | null;
  slot: HTMLElement | null;
  show(owner: string, meta: Meta, node?: ReactNode): void;
  hide(owner?: string): void;
}

const Ctx = createContext<Api | null>(null);

function useApi(): Api {
  const api = useContext(Ctx);
  if (!api) throw new Error("useInspector must be used inside the app shell.");
  return api;
}

export interface InspectorOpenOptions extends Meta {
  children: ReactNode;
}

// The host registers itself here so code outside React (a plugin) can open the panel too.
const bridge: { show: Api["show"] | null; hide: Api["hide"] | null } = { show: null, hide: null };

/** `inspector.open({ title, children })` / `inspector.close()`: the same panel, from anywhere (plugins get it as host.inspector). */
export const inspector = {
  open(options: InspectorOpenOptions) {
    bridge.show?.("imperative", { title: options.title, subtitle: options.subtitle, onClose: options.onClose }, options.children);
  },
  close() {
    bridge.hide?.();
  },
};

/** `open({ title, children })` / `close()` / `isOpen`, for content that is not tied to a component. */
export function useInspector() {
  const api = useApi();
  return useMemo(() => ({ open: inspector.open, close: inspector.close, isOpen: api.state !== null }), [api.state]);
}

/** Declarative inspector: renders `children` into the panel while `open`. */
export function Inspector({ open, onClose, title, subtitle, children }: { open: boolean; onClose?: () => void } & Meta & { children?: ReactNode }) {
  const api = useApi();
  const owner = useId();
  const { show, hide } = api;
  useEffect(() => {
    if (!open) return;
    show(owner, { title, subtitle, onClose });
  }, [open, owner, title, subtitle, onClose, show]);
  useEffect(() => () => hide(owner), [owner, hide]);
  useEffect(() => {
    if (!open) hide(owner);
  }, [open, owner, hide]);
  const live = open && api.state?.owner === owner;
  return live && api.slot ? createPortal(children, api.slot) : null;
}

export function InspectorHost({ children }: { children: ReactNode }) {
  const [state, setState] = useState<State | null>(null);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const show = useCallback(
    (owner: string, meta: Meta, node?: ReactNode) => {
      onCloseRef.current = meta.onClose;
      setState((s) => {
        const same = s && s.owner === owner && !node && !s.node && s.meta.title === meta.title && s.meta.subtitle === meta.subtitle;
        // Same panel, same words: keep the state so a page that re-renders with a fresh inline
        // callback does not re-render the host (the callback itself lives in a ref).
        return same ? s : { owner, meta, node };
      });
    },
    [],
  );
  const onCloseRef = useRef<(() => void) | undefined>(undefined);
  const hide = useCallback((owner?: string) => setState((s) => (s && (!owner || s.owner === owner) ? null : s)), []);
  const api = useMemo<Api>(() => ({ state, slot, show, hide }), [state, slot, show, hide]);
  useEffect(() => {
    bridge.show = show;
    bridge.hide = hide;
    return () => {
      bridge.show = null;
      bridge.hide = null;
    };
  }, [show, hide]);
  const close = () => {
    onCloseRef.current?.();
    hide();
  };
  return (
    <Ctx.Provider value={api}>
      <ResizablePanelGroup direction="horizontal" autoSaveId="neko-inspector" className="min-h-0 min-w-0 flex-1">
        <ResizablePanel id="page" order={1} minSize={35} className="min-h-0 min-w-0">
          <div className="flex h-full min-h-0 min-w-0 flex-col">{children}</div>
        </ResizablePanel>
        {state ? (
          <>
            <ResizableHandle withHandle aria-label="Resize the inspector" />
            <ResizablePanel id="inspector" order={2} defaultSize={34} minSize={22} maxSize={60} className="min-h-0 min-w-0">
              <aside
                data-slot="inspector"
                aria-label={typeof state.meta.title === "string" ? state.meta.title : "Inspector"}
                onKeyDown={(e) => {
                  if (e.key === "Escape" && !e.defaultPrevented) {
                    e.stopPropagation();
                    close();
                  }
                }}
                className="flex h-full min-h-0 flex-col bg-sidebar/60"
              >
                <header className="flex items-start gap-2 border-b border-border/70 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    {state.meta.subtitle ? <MonoLabel className="text-[10.5px]">{state.meta.subtitle}</MonoLabel> : null}
                    {state.meta.title ? <h2 className="truncate text-[15px] font-semibold text-strong">{state.meta.title}</h2> : null}
                  </div>
                  <Button type="button" variant="ghost" size="icon-sm" aria-label="Close the inspector" onClick={close}>
                    <X />
                  </Button>
                </header>
                <div data-slot="inspector-body" className="min-h-0 flex-1 overflow-y-auto">
                  <div ref={setSlot} />
                  {state.node}
                </div>
              </aside>
            </ResizablePanel>
          </>
        ) : null}
      </ResizablePanelGroup>
    </Ctx.Provider>
  );
}
