// from hushbacks packages/ui/src/components/sonner.tsx
import { Toast } from "@base-ui/react/toast";
import { CheckCircle, CircleNotch, Info, WarningCircle, X, XCircle } from "@/ui/icons";
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

type ToastAction = { label: React.ReactNode; onClick: () => void };
type ToastData = { action?: ToastAction; render?: (id: string) => React.ReactNode };

type CustomToastOptions = { id?: string; duration?: number; title?: string };

const toastManager = Toast.createToastManager<ToastData>();

type ToastOptions = {
  id?: string;
  description?: React.ReactNode;
  duration?: number;
  action?: ToastAction;
};

function emit(type: string | undefined, title: React.ReactNode, options?: ToastOptions): string {
  const data: ToastData | undefined = options?.action ? { action: options.action } : undefined;
  if (options?.id) {
    toastManager.update(options.id, { title, type, description: options.description, data });
    return options.id;
  }
  return toastManager.add({ title, type, description: options?.description, timeout: options?.duration, data });
}

const toast = Object.assign((title: React.ReactNode, options?: ToastOptions) => emit(undefined, title, options), {
  success: (title: React.ReactNode, options?: ToastOptions) => emit("success", title, options),
  error: (title: React.ReactNode, options?: ToastOptions) => emit("error", title, options),
  info: (title: React.ReactNode, options?: ToastOptions) => emit("info", title, options),
  warning: (title: React.ReactNode, options?: ToastOptions) => emit("warning", title, options),
  message: (title: React.ReactNode, options?: ToastOptions) => emit(undefined, title, options),
  loading: (title: React.ReactNode, options?: ToastOptions) =>
    options?.id
      ? emit("loading", title, options)
      : toastManager.add({ title, type: "loading", timeout: 0, description: options?.description }),
  custom: (render: (id: string) => React.ReactNode, options?: CustomToastOptions) =>
    toastManager.add({ id: options?.id, title: options?.title, timeout: options?.duration, data: { render } }),
  dismiss: (id?: string) => toastManager.close(id),
});

const TOAST_MOTION =
  "transition-[translate,scale,opacity] duration-(--motion-gentle) ease-(--ease-out) data-starting-style:-translate-y-4 data-starting-style:scale-[.97] data-starting-style:opacity-0 data-ending-style:-translate-y-2 data-ending-style:opacity-0 data-ending-style:duration-(--motion-quick) motion-reduce:transition-opacity motion-reduce:data-starting-style:translate-y-0 motion-reduce:data-starting-style:scale-100 motion-reduce:data-ending-style:translate-y-0";

const TYPE_ICON: Record<string, React.ReactNode> = {
  success: <CheckCircle weight="bold" className="size-4 text-emerald-500" />,
  error: <XCircle weight="bold" className="size-4 text-destructive" />,
  info: <Info weight="bold" className="size-4 text-sky-500" />,
  warning: <WarningCircle weight="bold" className="size-4 text-amber-500" />,
  loading: <CircleNotch weight="bold" className="size-4 animate-spin text-muted-foreground" />,
};

function ToastList() {
  const { toasts } = Toast.useToastManager();
  return toasts.map((item) => {
    const render = item.data?.render;
    if (render) {
      return (
        <Toast.Root key={item.id} toast={item} className={cn("relative", TOAST_MOTION)}>
          {item.title ? <Toast.Title className="sr-only" /> : null}
          {render(item.id)}
        </Toast.Root>
      );
    }
    const icon = item.type ? TYPE_ICON[item.type] : null;
    const action = item.data?.action;
    return (
      <Toast.Root
        key={item.id}
        toast={item}
        className={cn(
          "relative flex items-start gap-2.5 rounded-md bg-popover p-4 text-popover-foreground smooth-shadow-ring-md",
          TOAST_MOTION,
        )}
      >
        {icon ? <span className="mt-0.5 shrink-0">{icon}</span> : null}
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <Toast.Title className="text-sm font-medium" />
          <Toast.Description className="text-sm text-muted-foreground" />
          {action ? (
            <Toast.Action
              onClick={action.onClick}
              className="mt-1.5 w-fit rounded-md text-sm font-medium text-primary underline-offset-4 hover:underline"
            >
              {action.label}
            </Toast.Action>
          ) : null}
        </div>
        <Toast.Close
          className="shrink-0 rounded-sm text-muted-foreground transition-colors hover:text-foreground"
          aria-label="Close"
        >
          <X weight="bold" className="size-4" />
        </Toast.Close>
      </Toast.Root>
    );
  });
}

function Toaster(props: React.ComponentProps<typeof Toast.Viewport>) {
  return (
    <Toast.Provider toastManager={toastManager}>
      <Toast.Portal>
        <Toast.Viewport
          className={cn(
            "fixed top-4 left-1/2 z-[2000] flex w-80 max-w-[calc(100vw-2rem)] -translate-x-1/2 flex-col gap-2 outline-none",
            props.className,
          )}
          {...props}
        >
          <ToastList />
        </Toast.Viewport>
      </Toast.Portal>
    </Toast.Provider>
  );
}

export { Toaster, toast, toastManager };
