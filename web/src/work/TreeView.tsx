// All tasks as a tree: Neko on top, the watches and tasks it answers for, and
// the agent runs under each task (AgentTreeView.swift). An outline in the
// NSOutlineView / ARIA tree sense: one dense row per node, arrows move, Right
// opens a branch and Left closes it, Enter opens the task.
import { useCallback, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { ArrowBendUpLeft, CaretRight, Check, Clock, Eye, Hammer, ListChecks, ShieldCheck, Sparkle, Stop, WarningCircle, type Icon } from "@/ui/icons";
import { Binoculars, HandPalm, Lifebuoy, MapTrifold } from "@/work/icons";
import { Button } from "@/ui/button";
import { cn } from "@/ui/lib/utils";
import {
  descendants, expandsByDefault, formatCost, formatDuration, NODE_STATE_TITLE, treeSummary, visibleOrder,
  type AgentNode, type AgentTree, type NodeState,
} from "@/work/model";
import { StatusLabel, Tag, type Tone } from "@/work/tone";

const STATE_TONE: Record<NodeState, Tone> = {
  running: "running", needsYou: "attention", changesAsked: "attention", failing: "attention", review: "review",
  done: "done", passed: "done", watching: "done", failed: "failed", upNext: "neutral", stopped: "neutral", paused: "neutral",
};

const STATE_ICON: Record<NodeState, Icon> = {
  running: Check, upNext: Clock, needsYou: HandPalm, review: Eye, done: Check, passed: Check,
  changesAsked: ArrowBendUpLeft, stopped: Stop, paused: Stop, failed: WarningCircle, failing: WarningCircle, watching: Eye,
};

const RUN_ICON: Record<string, Icon> = {
  Scout: MapTrifold, Builder: Hammer, Reviewer: ShieldCheck, Recovery: Lifebuoy,
};

function nodeIcon(node: AgentNode) {
  if (node.kind === "neko") return Sparkle;
  if (node.kind === "watch") return Binoculars;
  if (node.kind === "task") return ListChecks;
  return RUN_ICON[node.name] ?? Check;
}

function accessibleName(node: AgentNode, expanded: boolean): string {
  const parts = [node.name];
  if (node.subtitle) parts.push(node.subtitle);
  parts.push(NODE_STATE_TITLE[node.state]);
  if (node.tools.length) parts.push(node.tools.join(", "));
  if (node.costUSD !== null) parts.push(formatCost(node.costUSD));
  if (node.durationMs !== null) parts.push(formatDuration(node.durationMs));
  if (node.state === "running" && node.work) parts.push(node.work);
  if (node.children.length) parts.push(`${node.children.length} below, ${expanded ? "shown" : "hidden"}`);
  return parts.join(", ");
}

interface Ctx {
  isExpanded: (n: AgentNode) => boolean;
  toggle: (n: AgentNode) => void;
  choose: (n: AgentNode) => void;
  focusId: string;
  setFocusId: (id: string) => void;
  onKey: (e: KeyboardEvent, n: AgentNode, parent: AgentNode | null) => void;
}

function Row({ node, parent, depth, ctx }: { node: AgentNode; parent: AgentNode | null; depth: number; ctx: Ctx }) {
  const expanded = ctx.isExpanded(node);
  const NodeIcon = nodeIcon(node);
  const StateIcon = STATE_ICON[node.state];
  const live = node.state === "running";
  const dimmed = node.kind === "run" && !live;
  const subtitle = node.subtitle || node.origin;
  return (
    <div className="flex items-stretch">
      {node.children.length ? (
        <button
          type="button"
          tabIndex={-1}
          onClick={() => ctx.toggle(node)}
          aria-label={expanded ? `Hide ${node.children.length} below ${node.name}` : `Show ${node.children.length} below ${node.name}`}
          title={expanded ? "Hide what it started" : "Show what it started"}
          className="grid h-9 w-7 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-foreground/[0.06] focus-visible:ring-2 focus-visible:ring-ring"
        >
          <CaretRight size={12} weight="bold" className={cn("transition-transform", expanded && "rotate-90")} aria-hidden />
        </button>
      ) : <span aria-hidden className="w-7 shrink-0" />}
      <div
        role="treeitem"
        data-slot="tree-row"
        data-node-id={node.id}
        data-kind={node.kind}
        aria-level={depth + 1}
        aria-expanded={node.children.length ? expanded : undefined}
        aria-label={accessibleName(node, expanded)}
        tabIndex={ctx.focusId === node.id ? 0 : -1}
        onFocus={() => ctx.setFocusId(node.id)}
        onClick={() => ctx.choose(node)}
        onKeyDown={(e) => ctx.onKey(e, node, parent)}
        title={node.origin ? `${node.origin}. Enter or click to open.` : "Enter or click to open."}
        className={cn(
          "flex h-9 min-w-0 flex-1 cursor-pointer items-center gap-2.5 rounded-lg pe-2 ps-1 text-[13.5px] outline-none",
          "hover:bg-foreground/[0.05] focus-visible:bg-foreground/[0.05] focus-visible:ring-2 focus-visible:ring-ring",
          dimmed && "opacity-70",
        )}
      >
        <span aria-hidden className="grid size-6 shrink-0 place-items-center rounded-md bg-foreground/[0.07] text-foreground/75">
          <NodeIcon size={13} />
        </span>
        <span className="min-w-0 flex-1 truncate">
          <span className="font-medium text-strong">{node.name}</span>
          {subtitle ? <span className="ms-2 text-[12.5px] text-muted-foreground">{subtitle}</span> : null}
          {live && node.work ? <span className="ms-2 text-[12.5px] text-muted-foreground/80">{node.work}</span> : null}
        </span>
        <span className="hidden shrink-0 items-center gap-1.5 md:flex">
          {node.tools.slice(0, 2).map((t) => <Tag key={t}>{t}</Tag>)}
          {node.tools.length > 2 ? <Tag>+{node.tools.length - 2}</Tag> : null}
        </span>
        <StatusLabel tone={STATE_TONE[node.state]} live={live} icon={<StateIcon weight="bold" aria-hidden />}>{NODE_STATE_TITLE[node.state]}</StatusLabel>
        <span className="hidden w-16 shrink-0 text-end font-mono text-[11.5px] tabular-nums text-muted-foreground sm:block">{node.costUSD !== null ? formatCost(node.costUSD) : ""}</span>
        <span className="hidden w-16 shrink-0 text-end font-mono text-[11.5px] tabular-nums text-muted-foreground sm:block">{node.durationMs !== null ? formatDuration(node.durationMs) : ""}</span>
      </div>
    </div>
  );
}

function Branch({ node, parent, depth, ctx }: { node: AgentNode; parent: AgentNode | null; depth: number; ctx: Ctx }) {
  const expanded = ctx.isExpanded(node) && node.children.length > 0;
  return (
    <li role="none" className="relative">
      <Row node={node} parent={parent} depth={depth} ctx={ctx} />
      {expanded ? (
        <ul role="group" className="ms-[13px]">
          {node.children.map((child) => (
            <li
              key={child.id}
              role="none"
              className="relative ps-4 before:absolute before:start-0 before:top-0 before:h-full before:w-px before:bg-border last:before:h-[18px] after:absolute after:start-0 after:top-[18px] after:h-px after:w-3 after:bg-border"
            >
              <ul role="none"><Branch node={child} parent={node} depth={depth + 1} ctx={ctx} /></ul>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

export function TreeView({ tree, onOpenTask, onOpenHome, onOpenWatching }: {
  tree: AgentTree;
  onOpenTask: (id: string) => void;
  onOpenHome: () => void;
  onOpenWatching: () => void;
}) {
  const [overrides, setOverrides] = useState<Record<string, boolean>>({});
  const [focusId, setFocusId] = useState(tree.root.id);
  const host = useRef<HTMLDivElement>(null);
  const isExpanded = useCallback((n: AgentNode) => overrides[n.id] ?? expandsByDefault(n), [overrides]);
  const toggle = useCallback((n: AgentNode) => {
    if (n.children.length) setOverrides((o) => ({ ...o, [n.id]: !(o[n.id] ?? expandsByDefault(n)) }));
  }, []);
  const choose = useCallback((n: AgentNode) => {
    if (n.kind === "neko") onOpenHome();
    else if (n.kind === "watch") onOpenWatching();
    else if (n.taskId) onOpenTask(n.taskId);
  }, [onOpenHome, onOpenTask, onOpenWatching]);

  const focusNode = (id: string) => {
    setFocusId(id);
    requestAnimationFrame(() => host.current?.querySelector<HTMLElement>(`[data-node-id="${CSS.escape(id)}"]`)?.focus());
  };

  const onKey = (e: KeyboardEvent, node: AgentNode, parent: AgentNode | null) => {
    const order = visibleOrder(tree.root, isExpanded);
    const at = order.indexOf(node.id);
    const open = isExpanded(node);
    switch (e.key) {
      case "ArrowDown": if (order[at + 1]) focusNode(order[at + 1]); break;
      case "ArrowUp": if (order[at - 1]) focusNode(order[at - 1]); break;
      case "Home": focusNode(order[0]); break;
      case "End": focusNode(order[order.length - 1]); break;
      case "ArrowRight":
        if (node.children.length && !open) toggle(node);
        else if (node.children.length) focusNode(node.children[0].id);
        break;
      case "ArrowLeft":
        if (node.children.length && open) toggle(node);
        else if (parent) focusNode(parent.id);
        break;
      case "Enter": case " ": choose(node); break;
      default: return;
    }
    e.preventDefault();
  };

  const everything = useMemo(() => descendants(tree.root).filter((n) => n.children.length), [tree]);
  const allOpen = everything.every((n) => isExpanded(n));
  const setAll = (open: boolean) => setOverrides(Object.fromEntries(everything.map((n) => [n.id, open])));

  const ctx: Ctx = { isExpanded, toggle, choose, focusId, setFocusId, onKey };
  return (
    <div ref={host} data-slot="agent-tree" className="w-full px-3.5 pt-2 pb-10">
      <div className="mb-2 flex items-center gap-2 ps-7">
        <span data-slot="tree-summary" className="font-mono text-[12px] tabular-nums text-muted-foreground" title="What runs reported spending. Codex runs report no cost, so they are not counted.">
          {treeSummary(tree)}
        </span>
        <span className="flex-1" />
        {everything.length ? (
          <Button variant="ghost" size="xs" className="text-muted-foreground" onClick={() => setAll(!allOpen)}>
            {allOpen ? "Collapse all" : "Expand all"}
          </Button>
        ) : null}
      </div>
      <ul role="tree" aria-label="Who started whom">
        <Branch node={tree.root} parent={null} depth={0} ctx={ctx} />
      </ul>
    </div>
  );
}
