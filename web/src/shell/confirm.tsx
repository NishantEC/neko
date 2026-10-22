// One confirmation dialog for the whole app. Anything destructive asks first:
//
//   if (await confirm({ title: "Remove the plugin?", description: "…", action: "Remove", destructive: true })) { … }
//
// `ConfirmHost` (mounted once by AppShell) shows it. Esc and Cancel answer false.
import { useEffect, useState } from "react";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/ui/alert-dialog";

export interface ConfirmOptions {
  title: string;
  description?: React.ReactNode;
  /** The button that goes ahead. Name the thing: "Stop all work", not "OK". */
  action: string;
  cancel?: string;
  destructive?: boolean;
}

type Pending = ConfirmOptions & { resolve(answer: boolean): void };
let show: ((pending: Pending) => void) | null = null;

export function confirm(options: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    if (!show) return resolve(false);
    show({ ...options, resolve });
  });
}

export function ConfirmHost() {
  const [pending, setPending] = useState<Pending | null>(null);
  useEffect(() => {
    show = (next) => setPending((current) => {
      current?.resolve(false);
      return next;
    });
    return () => {
      show = null;
    };
  }, []);
  const answer = (value: boolean) => {
    pending?.resolve(value);
    setPending(null);
  };
  return (
    <AlertDialog open={pending !== null} onOpenChange={(open) => !open && answer(false)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{pending?.title}</AlertDialogTitle>
          {pending?.description ? <AlertDialogDescription>{pending.description}</AlertDialogDescription> : null}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={() => answer(false)}>{pending?.cancel ?? "Cancel"}</AlertDialogCancel>
          <AlertDialogAction
            data-slot="confirm-action"
            onClick={() => answer(true)}
            variant={pending?.destructive ? "destructive" : "default"}
          >
            {pending?.action}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
