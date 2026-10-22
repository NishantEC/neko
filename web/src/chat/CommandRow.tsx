// One command an agent ran, as a T3 Code work-log entry (chat/t3/TimelineWorkRows.tsx,
// from t3code @3b95e7831, MIT): the command on a line, the output on demand.
import { SimpleWorkEntryRow } from "@/chat/t3/TimelineWorkRows";
import type { Receipt } from "@/work/model";

export function CommandRow({ receipt }: { receipt: Receipt }) {
  const output = receipt.output === null ? "Output was not saved." : receipt.output === "" ? "No output." : receipt.output;
  return (
    <div data-slot="command-row" data-failed={receipt.failed}>
      <SimpleWorkEntryRow
        slot="command-entry"
        label={<span className="font-mono text-[0.92em]">{receipt.title}</span>}
        aside={receipt.exitCode ? `exit ${receipt.exitCode}` : receipt.incomplete ? "incomplete" : undefined}
        iconName={receipt.incomplete ? "message-circle" : "terminal"}
        failed={receipt.failed}
        detail={[...receipt.notices, output].join("\n\n")}
      />
    </div>
  );
}
