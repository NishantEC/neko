// from t3code apps/web/src/components/chat/ComposerPendingApprovalPanel.tsx and
// ComposerPendingApprovalActions.tsx, and the approval drawer in
// ChatComposer.tsx @3b95e7831 (MIT)
//
// Adapted: one Neko decision (a tool call to approve, or an action Neko
// proposed) instead of T3's provider approval request; two answers, Decline and
// Approve, with Neko's keys on them. The count reads "1 of N".
import { memo, type ReactNode } from "react";
import { ShieldIcon } from "lucide-react";
import { Button } from "@/ui/button";
import { Kbd } from "@/ui/kbd";
import { cn } from "@/ui/lib/utils";
import { ComposerBanner } from "./ComposerBanner";

export const ComposerPendingApprovalPanel = memo(function ComposerPendingApprovalPanel({
  label, subject, detail, pendingCount, context, className,
}: {
  label: string;
  subject: string;
  detail: string;
  pendingCount: number;
  context?: ReactNode;
  className?: string;
}) {
  return (
    <span aria-label={label} className={cn("flex min-w-0 flex-1 flex-col items-start gap-1", className)} role="group">
      <span className="flex w-full min-w-0 items-center gap-2 text-2xs text-muted-foreground">
        <span className="shrink-0 font-medium text-warning">{label}</span>
        {context ? <span className="min-w-0 truncate">{context}</span> : null}
        {pendingCount > 1 ? <span className="ml-auto shrink-0 tabular-nums">1 of {pendingCount}</span> : null}
      </span>
      <span className="text-sm font-medium text-foreground">{subject}</span>
      {detail ? (
        <code
          aria-label="What it will do"
          className="block max-h-40 w-full min-w-0 overflow-auto whitespace-pre-wrap font-mono text-xs text-foreground wrap-anywhere [scrollbar-width:thin] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/70 [&::-webkit-scrollbar]:h-1.5"
          data-approval-detail="complete"
          tabIndex={0}
        >
          {detail}
        </code>
      ) : null}
    </span>
  );
});

export const ComposerPendingApprovalActions = memo(function ComposerPendingApprovalActions({
  isResponding, onDecide, approveKey,
}: {
  isResponding: boolean;
  onDecide: (approve: boolean) => void;
  approveKey: string;
}) {
  return (
    <>
      <Button size="xs" variant="outline" className="h-7 gap-1.5 px-2.5" disabled={isResponding} onClick={() => onDecide(false)}>
        <span className="max-w-40 truncate">Decline</span>
        <Kbd className="h-4 min-w-4 px-1 text-[10px]">Esc</Kbd>
      </Button>
      <Button size="xs" variant="default" className="h-7 gap-1.5 px-2.5" disabled={isResponding} onClick={() => onDecide(true)}>
        <span className="max-w-40 truncate">Approve</span>
        <Kbd className="h-4 min-w-4 bg-white/15 px-1 text-[10px] text-primary-foreground">{approveKey}</Kbd>
      </Button>
    </>
  );
});

/** The warning drawer on the composer's top edge, holding one decision. */
export function ComposerApprovalDrawer({
  children, actions, error, ...rest
}: { children: ReactNode; actions: ReactNode; error?: string | null } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <ComposerBanner.Attachment>
      <ComposerBanner.Root variant="warning" density="spacious" {...rest}>
        <ComposerBanner.Row layout="approval">
          <ComposerBanner.Icon>
            <ShieldIcon />
          </ComposerBanner.Icon>
          <ComposerBanner.Content>{children}</ComposerBanner.Content>
          <ComposerBanner.Actions>{actions}</ComposerBanner.Actions>
        </ComposerBanner.Row>
        {error ? <ComposerBanner.Body className="pt-2 text-xs text-destructive">{error}</ComposerBanner.Body> : null}
      </ComposerBanner.Root>
    </ComposerBanner.Attachment>
  );
}
