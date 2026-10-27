// /#/ui — every vendored primitive, editorial and chat component, with
// realistic Neko data, in whichever theme is active (switch it in the sidebar
// or with ⌘K). This is how agents verify design work: scripts/web-verify.mjs
// screenshots each `data-gallery-section` in light and dark.
//
// Adding a component to the design system means adding it here too.
import { useState } from "react";
import {
  Accordion, AccordionItem, AccordionPanel, AccordionTrigger,
} from "@/ui/accordion";
import { Alert, AlertDescription, AlertTitle } from "@/ui/alert";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter,
  AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger,
} from "@/ui/alert-dialog";
import { Avatar, AvatarFallback, AvatarGroup } from "@/ui/avatar";
import { Badge } from "@/ui/badge";
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "@/ui/breadcrumb";
import { Button } from "@/ui/button";
import { ButtonGroup, ButtonGroupText } from "@/ui/button-group";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/ui/card";
import { BarChart, LineChart } from "@/ui/charts";
import { Checkbox } from "@/ui/checkbox";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/ui/collapsible";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/ui/command";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from "@/ui/context-menu";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/ui/dialog";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuTrigger,
} from "@/ui/dropdown-menu";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/ui/empty";
import { Field, FieldDescription, FieldLabel as FormLabel } from "@/ui/field";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui/hover-card";
import * as Icons from "@/ui/icons";
import { GitDiff } from "@phosphor-icons/react";
import { DockWorkspace } from "@/work/dock/DockWorkspace";
import { ThemeMenu } from "@/shell/StatusBar";
import { Input } from "@/ui/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui/input-group";
import { Kbd, KbdGroup } from "@/ui/kbd";
import { Label } from "@/ui/label";
import { Popover, PopoverContent, PopoverDescription, PopoverHeader, PopoverTitle, PopoverTrigger } from "@/ui/popover";
import { Progress } from "@/ui/progress";
import { RadioGroup, RadioGroupItem } from "@/ui/radio-group";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/ui/resizable";
import { ScrollArea } from "@/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui/select";
import { Separator } from "@/ui/separator";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle, SheetTrigger } from "@/ui/sheet";
import { Skeleton } from "@/ui/skeleton";
import { toast } from "@/ui/sonner";
import { Spinner } from "@/ui/spinner";
import { StatusBadge } from "@/ui/status-badge";
import { Switch } from "@/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/ui/table";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/ui/tabs";
import { Textarea } from "@/ui/textarea";
import { Timeline, TimelineContent, TimelineDate, TimelineHeader, TimelineIndicator, TimelineItem, TimelineSeparator, TimelineTitle } from "@/ui/timeline";
import { Toggle } from "@/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { EditorialHeading, FieldLabel, GlowTile, MonoLabel, PageHeader, Panel, SectionHead } from "@/editorial";
import { Attachment, AttachmentInfo, AttachmentPreview, Attachments, AttachmentRemove } from "@/chat/attachments";
import { ChatInput } from "@/chat/chat-input";
import { Conversation, ConversationContent, ConversationEmptyState } from "@/chat/conversation";
import { DecisionCard } from "@/chat/decisions";
import { JsonViewer } from "@/chat/json-viewer";
import { Message, MessageAction, MessageActions, MessageContent, MessageResponse } from "@/chat/message";
import { QueueTray } from "@/chat/queue-tray";
import { Suggestion, Suggestions } from "@/chat/suggestion";
import { NekoTurn, UserTurn } from "@/chat/turns";
import { ToolFailed, ToolFold, ToolLine, WorkLog } from "@/chat/work-log";
import { splitTurn } from "@/chat/sdk/parts";
import { cn } from "@/ui/lib/utils";
import { SidebarOptions } from "@/pages/gallery/SidebarOptions";
import { finishedReply, prRows, queued, runningReply, sampleArgs, structuredReply, usage, userMessage } from "@/pages/gallery-sample";

function Section({ id, eyebrow, title, children }: { id: string; eyebrow: string; title: React.ReactNode; children: React.ReactNode }) {
  return (
    <section data-gallery-section={id} aria-labelledby={`g-${id}`} className="flex flex-col gap-6 scroll-mt-6">
      <div className="flex flex-col gap-2 border-b border-border pb-4">
        <MonoLabel>{eyebrow}</MonoLabel>
        <EditorialHeading as="h2" className="text-[clamp(1.5rem,2vw+0.5rem,2rem)]" style={{}}>
          <span id={`g-${id}`}>{title}</span>
        </EditorialHeading>
      </div>
      {children}
    </section>
  );
}

function Specimen({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-2.5">
      <FieldLabel>{label}</FieldLabel>
      <div className={cn("flex flex-wrap items-center gap-2.5", className)}>{children}</div>
    </div>
  );
}

const SWATCHES = [
  "background", "card", "popover", "primary", "secondary", "muted", "accent", "destructive", "success", "warning", "info", "bubble", "sidebar", "border",
] as const;

function Foundations() {
  return (
    <Section id="foundations" eyebrow="Foundations" title={<>Type, colour &amp; light.</>}>
      <div className="grid items-start gap-8 md:grid-cols-2">
        <Specimen label="Three voices" className="flex-col items-start">
          <MonoLabel>Eyebrow · mono</MonoLabel>
          <EditorialHeading as="p" className="text-[2rem]">Who is waiting.</EditorialHeading>
          <p className="text-muted-foreground">Body in Inter, muted. Numbers are tabular: 1,284 tokens · 12:48.</p>
          <code className="rounded-md bg-foreground/[0.05] px-1.5 py-0.5 text-[12px]">gh pr view 4641 --json reviews</code>
        </Specimen>
        <Specimen label="Tokens">
          <div className="grid w-full grid-cols-4 gap-2 sm:grid-cols-7">
            {SWATCHES.map((s) => (
              <div key={s} className="flex flex-col gap-1">
                <div className="h-9 rounded-lg border border-border" style={{ background: `var(--${s})` }} />
                <span className="font-mono text-[10px] text-muted-foreground">{s}</span>
              </div>
            ))}
          </div>
        </Specimen>
        <Specimen label="Smooth shadows">
          {(["xs", "sm", "md", "lg", "xl"] as const).map((s) => (
            <div key={s} className={cn("grid size-16 place-items-center rounded-xl bg-card font-mono text-xs ring-1 ring-border", `shadow-${s}`)}>{s}</div>
          ))}
        </Specimen>
        <Specimen label="Surfaces">
          <div className="ui-panel-surface border border-border bg-card px-4 py-3 text-sm">ui-panel-surface</div>
          <div className="ui-popup-surface bg-popover px-4 py-3 text-sm">ui-popup-surface</div>
          <div className="ui-inset-control border px-4 py-2 text-sm">ui-inset-control</div>
          <StatusBadge>neutral</StatusBadge>
          <StatusBadge tone="success">success</StatusBadge>
          <StatusBadge tone="warning">warning</StatusBadge>
          <StatusBadge tone="error">error</StatusBadge>
        </Specimen>
      </div>
    </Section>
  );
}

function Controls() {
  const [on, setOn] = useState(true);
  return (
    <Section id="controls" eyebrow="Primitives" title={<>Buttons &amp; badges.</>}>
      <div className="grid items-start gap-8 md:grid-cols-2">
        <Specimen label="Button variants">
          <Button>Approve</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="outline">Decline</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="destructive">Stop task</Button>
          <Button variant="link">Link</Button>
          <Button loading>Saving</Button>
          <Button disabled>Disabled</Button>
        </Specimen>
        <Specimen label="Sizes and icon buttons">
          <Button size="xs">xs</Button>
          <Button size="sm">sm</Button>
          <Button>default</Button>
          <Button size="lg">lg</Button>
          <Button size="icon" variant="outline" aria-label="Copy"><Icons.Copy /></Button>
          <Button size="icon-sm" variant="ghost" aria-label="More"><Icons.DotsThree /></Button>
          <Button size="icon-xs" variant="ghost" aria-label="Close"><Icons.X /></Button>
        </Specimen>
        <Specimen label="Button group">
          <ButtonGroup>
            <Button variant="outline">Merge</Button>
            <Button variant="outline">Squash</Button>
            <ButtonGroupText>3 checks</ButtonGroupText>
          </ButtonGroup>
        </Specimen>
        <Specimen label="Badges and keys">
          <Badge>New</Badge>
          <Badge variant="secondary">Draft</Badge>
          <Badge variant="outline">Neko decides</Badge>
          <Badge variant="destructive">Failed</Badge>
          <KbdGroup><Kbd>⌘</Kbd><Kbd>K</Kbd></KbdGroup>
          <Kbd>Esc</Kbd>
        </Specimen>
        <Specimen label="Toggle, switch, spinner">
          <Toggle aria-label="Bold" variant="outline"><Icons.TextB /></Toggle>
          <ToggleGroup defaultValue={["list"]} variant="outline">
            <ToggleGroupItem value="list" aria-label="List"><Icons.ListBullets /></ToggleGroupItem>
            <ToggleGroupItem value="board" aria-label="Board"><Icons.Columns /></ToggleGroupItem>
          </ToggleGroup>
          <Label className="flex items-center gap-2">
            <Switch checked={on} onCheckedChange={setOn} /> Run unattended
          </Label>
          <Spinner />
        </Specimen>
        <Specimen label="Progress">
          <Progress value={64} className="w-64" aria-label="Budget used" />
        </Specimen>
      </div>
    </Section>
  );
}

function Forms() {
  return (
    <Section id="forms" eyebrow="Primitives" title={<>Forms &amp; fields.</>}>
      <div className="grid max-w-3xl gap-6 md:grid-cols-2">
        <Field>
          <FormLabel htmlFor="g-name">Task title</FormLabel>
          <Input id="g-name" defaultValue="Update athena #4641 branch" />
          <FieldDescription>Shown on the board and in the PR title.</FieldDescription>
        </Field>
        <Field>
          <FormLabel htmlFor="g-workspace">Workspace</FormLabel>
          <Select defaultValue="athena">
            <SelectTrigger id="g-workspace" className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="athena">athena</SelectItem>
              <SelectItem value="riavox">riavox</SelectItem>
              <SelectItem value="neko">neko</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field className="md:col-span-2">
          <FormLabel htmlFor="g-note">Note to its agent</FormLabel>
          <Textarea id="g-note" placeholder="Rebase onto master, then rerun the checks." />
        </Field>
        <InputGroup className="md:col-span-2">
          <InputGroupInput placeholder="Search tasks" aria-label="Search tasks" />
          <InputGroupAddon><Icons.MagnifyingGlass /></InputGroupAddon>
        </InputGroup>
        <div className="flex flex-col gap-3">
          <Label className="flex items-center gap-2"><Checkbox defaultChecked /> Run the tests first</Label>
          <Label className="flex items-center gap-2"><Checkbox /> Open a draft PR</Label>
        </div>
        <RadioGroup defaultValue="plan" aria-label="Mode">
          <Label className="flex items-center gap-2"><RadioGroupItem value="plan" /> Plan first</Label>
          <Label className="flex items-center gap-2"><RadioGroupItem value="build" /> Build straight away</Label>
        </RadioGroup>
      </div>
    </Section>
  );
}

function Overlays() {
  return (
    <Section id="overlays" eyebrow="Primitives" title={<>Overlays &amp; menus.</>}>
      <div className="grid items-start gap-8 md:grid-cols-2">
        <Specimen label="Dialog · alert dialog · sheet">
          <Dialog>
            <DialogTrigger asChild><Button variant="outline">Dialog</Button></DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Rename the task</DialogTitle>
                <DialogDescription>The new name is used for the branch and the pull request.</DialogDescription>
              </DialogHeader>
              <Input defaultValue="Update athena #4641 branch" aria-label="Task name" />
              <DialogFooter><Button>Save</Button></DialogFooter>
            </DialogContent>
          </Dialog>
          <AlertDialog>
            <AlertDialogTrigger asChild><Button variant="destructive-outline">Close pull request</Button></AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Close athena #4641?</AlertDialogTitle>
                <AlertDialogDescription>It stays on GitHub as closed. You can reopen it there.</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Keep open</AlertDialogCancel>
                <AlertDialogAction>Close it</AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          <Sheet>
            <SheetTrigger asChild><Button variant="outline">Sheet</Button></SheetTrigger>
            <SheetContent>
              <SheetHeader>
                <SheetTitle>Task details</SheetTitle>
                <SheetDescription>Everything Neko knows about this task.</SheetDescription>
              </SheetHeader>
            </SheetContent>
          </Sheet>
        </Specimen>
        <Specimen label="Menus">
          <DropdownMenu>
            <DropdownMenuTrigger asChild><Button variant="outline">Actions <Icons.CaretDown /></Button></DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem>Start <DropdownMenuShortcut>⌘↵</DropdownMenuShortcut></DropdownMenuItem>
              <DropdownMenuItem>Run next</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive">Stop</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <ContextMenu>
            <ContextMenuTrigger className="grid h-9 place-items-center rounded-lg border border-dashed border-border px-4 text-[13px] text-muted-foreground">Right-click me</ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem>Copy link</ContextMenuItem>
              <ContextMenuItem>Open on GitHub</ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
        </Specimen>
        <Specimen label="Popover · hover card · tooltip · toast">
          <Popover>
            <PopoverTrigger asChild><Button variant="outline">Popover</Button></PopoverTrigger>
            <PopoverContent>
              <PopoverHeader>
                <PopoverTitle>Budget</PopoverTitle>
                <PopoverDescription>$4.20 of $10 used today.</PopoverDescription>
              </PopoverHeader>
            </PopoverContent>
          </Popover>
          <HoverCard>
            <HoverCardTrigger asChild><Button variant="link">athena #4641</Button></HoverCardTrigger>
            <HoverCardContent>Add coach tools for report review calls. Checks passing, behind master.</HoverCardContent>
          </HoverCard>
          <Tooltip>
            <TooltipTrigger asChild><Button variant="ghost" size="icon" aria-label="Info"><Icons.Info /></Button></TooltipTrigger>
            <TooltipContent>Neko decides the model</TooltipContent>
          </Tooltip>
          <Button variant="outline" onClick={() => toast.success("Pull request merged", { description: "athena #4628 was squashed into master." })}>Toast</Button>
        </Specimen>
        <Specimen label="Command (⌘K)" className="items-stretch">
          <Command className="h-52 w-full max-w-sm rounded-xl border border-border">
            <CommandInput placeholder="Go to a page or run an action…" aria-label="Palette demo" />
            <CommandList>
              <CommandEmpty>Nothing matches.</CommandEmpty>
              <CommandGroup heading="Go to">
                <CommandItem><Icons.House /> Go to Home</CommandItem>
                <CommandItem><Icons.GitPullRequest /> Go to PR board <CommandShortcut>yours</CommandShortcut></CommandItem>
              </CommandGroup>
            </CommandList>
          </Command>
        </Specimen>
      </div>
    </Section>
  );
}

function DataDisplay() {
  return (
    <Section id="data" eyebrow="Primitives" title={<>Data &amp; layout.</>}>
      <div className="lg:columns-2 lg:gap-8 [&>*]:mb-8 [&>*]:break-inside-avoid">
        <Specimen label="Table" className="w-full">
          <div className="w-full overflow-x-auto rounded-xl border border-border">
            <Table>
              <TableHeader>
                <TableRow><TableHead>PR</TableHead><TableHead>Task</TableHead><TableHead>Checks</TableHead></TableRow>
              </TableHeader>
              <TableBody>
                {prRows.map((r) => (
                  <TableRow key={r.ref}>
                    <TableCell className="font-mono text-xs">{r.ref}</TableCell>
                    <TableCell className="max-w-44 truncate">{r.task}</TableCell>
                    <TableCell><StatusBadge tone={r.checks === "passing" ? "success" : "error"}>{r.checks}</StatusBadge></TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </Specimen>
        <Specimen label="Tabs" className="w-full items-start">
          <Tabs defaultValue="plan" className="w-full">
            <TabsList><TabsTab value="plan">Plan</TabsTab><TabsTab value="changes">Changes</TabsTab><TabsTab value="review">Review</TabsTab></TabsList>
            <TabsPanel value="plan" className="pt-3 text-muted-foreground">A scout reads the repo and proposes the change.</TabsPanel>
            <TabsPanel value="changes" className="pt-3 text-muted-foreground">12 files, +318 −97.</TabsPanel>
            <TabsPanel value="review" className="pt-3 text-muted-foreground">An independent reviewer reads the diff.</TabsPanel>
          </Tabs>
        </Specimen>
        <Specimen label="Accordion · collapsible" className="w-full items-start">
          <Accordion className="w-full">
            <AccordionItem value="a"><AccordionTrigger>What does Neko remember?</AccordionTrigger><AccordionPanel>Working-style preferences you confirmed, never secrets.</AccordionPanel></AccordionItem>
            <AccordionItem value="b"><AccordionTrigger>Who can approve a merge?</AccordionTrigger><AccordionPanel>Only a click from you.</AccordionPanel></AccordionItem>
          </Accordion>
          <Collapsible className="w-full">
            <CollapsibleTrigger render={<Button variant="ghost" size="sm" />}>Show 3 earlier steps <Icons.CaretDown /></CollapsibleTrigger>
            <CollapsibleContent className="px-2 py-2 text-[13px] text-muted-foreground">Read neko_chat.rs · Searched once · Ran 3 commands</CollapsibleContent>
          </Collapsible>
        </Specimen>
        <Specimen label="Timeline" className="w-full items-start">
          <Timeline defaultValue={3} className="w-full">
            {[
              ["Planned", "13:02"], ["Building", "13:09"], ["Reviewing", "13:31"], ["Ready for review", "13:40"],
            ].map(([title, at], i) => (
              <TimelineItem key={title} step={i + 1}>
                <TimelineHeader>
                  <TimelineSeparator />
                  <TimelineDate>{at}</TimelineDate>
                  <TimelineTitle>{title}</TimelineTitle>
                  <TimelineIndicator />
                </TimelineHeader>
                <TimelineContent>{i === 3 ? "Waiting for you." : "Done."}</TimelineContent>
              </TimelineItem>
            ))}
          </Timeline>
        </Specimen>
        <Specimen label="Breadcrumb · avatars · separator">
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem><BreadcrumbLink href="#/">Home</BreadcrumbLink></BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem><BreadcrumbPage>PR board</BreadcrumbPage></BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
          <Separator orientation="vertical" className="h-5" />
          <AvatarGroup>
            <Avatar><AvatarFallback>NG</AvatarFallback></Avatar>
            <Avatar><AvatarFallback>AT</AvatarFallback></Avatar>
          </AvatarGroup>
        </Specimen>
        <Specimen label="Alert · empty · skeleton" className="w-full items-start">
          <Alert className="w-full">
            <Icons.Warning />
            <AlertTitle>The budget is nearly used</AlertTitle>
            <AlertDescription>Neko will stop starting new work at 100%.</AlertDescription>
          </Alert>
          <Empty className="w-full border border-dashed border-border">
            <EmptyHeader>
              <EmptyMedia variant="icon"><Icons.Tray /></EmptyMedia>
              <EmptyTitle>Nothing is waiting on you</EmptyTitle>
              <EmptyDescription>Approvals and questions from your agents show up here.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent><Button variant="outline" size="sm">Ask Neko</Button></EmptyContent>
          </Empty>
          <div className="flex w-full flex-col gap-2"><Skeleton className="h-4 w-1/3" /><Skeleton className="h-4 w-full" /><Skeleton className="h-4 w-2/3" /></div>
        </Specimen>
        <Specimen label="Card · scroll area" className="w-full items-start">
          <Card className="w-full">
            <CardHeader><CardTitle>Budget today</CardTitle><CardDescription>Resets at midnight.</CardDescription></CardHeader>
            <CardContent><Progress value={42} aria-label="Budget used" /></CardContent>
            <CardFooter className="text-[13px] text-muted-foreground">$4.20 of $10</CardFooter>
          </Card>
          <ScrollArea className="h-24 w-full rounded-xl border border-border">
            <div className="space-y-1 p-3 text-[13px] text-muted-foreground">
              {Array.from({ length: 12 }, (_, i) => <p key={i}>Event {i + 1}: task moved to Ready for review</p>)}
            </div>
          </ScrollArea>
        </Specimen>
        <Specimen label="Charts (recharts)" className="w-full items-start">
          <div className="grid w-full gap-4 [&>*]:min-w-0">
            <BarChart data={usage} dataKey="tokens" labelKey="day" chartConfig={{ tokens: { label: "Tokens (k)", color: "var(--chart-1)" } }} emptyMessage="No usage yet" height="150px" />
            <LineChart data={usage} dataKey="tokens" xKey="day" chartConfig={{ tokens: { label: "Tokens (k)", color: "var(--chart-2)" } }} height="150px" />
          </div>
        </Specimen>
        <Specimen label="Resizable panels" className="w-full">
          <ResizablePanelGroup direction="horizontal" className="h-24 w-full rounded-xl border border-border">
            <ResizablePanel defaultSize={35}><div className="grid h-full place-items-center text-[13px] text-muted-foreground">Tasks</div></ResizablePanel>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={65}><div className="grid h-full place-items-center text-[13px] text-muted-foreground">Task chat</div></ResizablePanel>
          </ResizablePanelGroup>
        </Specimen>
      </div>
    </Section>
  );
}

function Editorial() {
  return (
    <Section id="editorial" eyebrow="Editorial" title={<>One panel, divided into rows.</>}>
      <div className="grid items-start gap-8 lg:grid-cols-2">
        <div className="flex flex-col gap-8">
          <PageHeader eyebrow="Settings · Budget" title={<>Spend, kept honest.</>} subtitle="What Neko may spend before it asks you." />
          <SectionHead eyebrow="Watching" title={<>Things Neko keeps an eye on.</>} intro="A watch checks a source on a schedule and files a task when something needs doing." />
        </div>
        <Panel className="max-w-2xl">
          <div className="px-5 py-4"><MonoLabel>Daily budget</MonoLabel><p className="mt-1 text-strong">$10.00 per day</p></div>
          <div className="divide-y divide-border/70 border-t border-border/70">
            <div className="flex items-center justify-between px-5 py-4"><FieldLabel>Writable runs per repository</FieldLabel><span className="font-mono text-sm">4</span></div>
            <div className="flex items-center justify-between px-5 py-4"><FieldLabel>Plan before working</FieldLabel><Switch defaultChecked aria-label="Plan before working" /></div>
          </div>
          <div className="flex justify-end border-t border-border/70 bg-foreground/[0.02] px-5 py-4"><Button>Save changes</Button></div>
        </Panel>
        <GlowTile glowColor="var(--glow-violet)" className="p-5" containerClassName="min-h-32">
          <MonoLabel>Waiting for you</MonoLabel>
          <p className="mt-2 text-2xl font-medium text-strong tabular-nums">3 approvals</p>
          <p className="mt-1 text-[13px] text-muted-foreground">Oldest: save_issue, 4 minutes ago.</p>
        </GlowTile>
        <GlowTile glowColor="var(--glow-emerald)" glowCorner="bl" onClick={() => toast("Opened the PR board")} className="p-5" containerClassName="min-h-32">
          <MonoLabel>Pull requests</MonoLabel>
          <p className="mt-2 text-2xl font-medium text-strong tabular-nums">5 ready</p>
          <p className="mt-1 text-[13px] text-muted-foreground">All checks passing. Click to open the board.</p>
        </GlowTile>
      </div>
    </Section>
  );
}

function ChatSection() {
  const [text, setText] = useState("");
  const [open] = useState(true);
  return (
    <Section id="chat" eyebrow="Chat" title={<>The conversation, piece by piece.</>}>
      <div className="grid items-start gap-10 lg:grid-cols-2">
        <div className="flex flex-col gap-3">
          <FieldLabel>Turns: user bubble, finished reply (work log, tool call, table), live reply</FieldLabel>
          <div className="rounded-2xl border border-border bg-background px-6 pb-6">
            <UserTurn message={userMessage} />
            <NekoTurn message={finishedReply} />
            <NekoTurn message={runningReply} />
          </div>
          <FieldLabel>Reply blocks: a question, a plan, a chart and a form the model can ask for</FieldLabel>
          <div data-slot="reply-blocks" className="rounded-2xl border border-border bg-background px-6 pb-6">
            <NekoTurn message={structuredReply} />
          </div>
        </div>
        <div className="flex flex-col gap-8">
          <Specimen label="WorkLog (live · done)" className="flex-col items-stretch">
            <WorkLog message={runningReply} work={splitTurn(runningReply.parts).work} />
            <WorkLog message={finishedReply} work={splitTurn(finishedReply.parts).work} />
          </Specimen>
          <Specimen label="ToolLine · ToolFold · ToolFailed" className="flex-col items-stretch gap-0">
            <ToolLine>Updated the budget to $10 a day</ToolLine>
            <ToolLine running>Reading the repository</ToolLine>
            <ToolFold label="Called save_issue" aside="linear · done" defaultOpen>
              <JsonViewer data={sampleArgs} rootName="arguments" className="text-[11.5px]" />
            </ToolFold>
            <ToolFailed what="get_pull_request did not finish" detail="github · the token expired" />
          </Specimen>
        </div>
      </div>
      <div className="grid items-start gap-8 lg:grid-cols-2">
        <Specimen label="DecisionCard" className="flex-col items-stretch">
          <DecisionCard
            decision={{ key: "d", label: "Tool approval · linear", subject: "save_issue", detail: JSON.stringify(sampleArgs, null, 2), args: sampleArgs, decide: async () => ({}) }}
            count={4}
            onDecide={() => {}}
          />
        </Specimen>
        <Specimen label="ChatInput with queue tray (paste an image into it)" className="flex-col items-stretch">
          <ChatInput
            text={text}
            onTextChange={setText}
            onSendMessage={() => setText("")}
            isGenerating
            allowImages
            placeholder="Queue a message while you decide…"
            leading={<Badge variant="outline" className="rounded-full text-[12.5px] font-normal text-muted-foreground">Neko decides</Badge>}
            tray={<QueueTray queued={queued} onEdit={() => {}} onSteer={() => {}} onRemove={() => {}} />}
          />
        </Specimen>
        <Specimen label="Message · actions · response" className="flex-col items-stretch">
          <Message from="assistant">
            <MessageContent><MessageResponse>{"Two **blocked** pull requests: `athena #4641` is behind master, `riavox #410` has a failing check."}</MessageResponse></MessageContent>
            <MessageActions><MessageAction tooltip="Copy" size="icon-xs"><Icons.Copy size={14} /></MessageAction></MessageActions>
          </Message>
        </Specimen>
        <Specimen label="Suggestions · attachments" className="flex-col items-stretch">
          <Suggestions>
            <Suggestion suggestion="What needs my attention right now?" />
            <Suggestion suggestion="Which pull requests are ready to review?" />
          </Suggestions>
          <Attachments variant="inline" className="ml-0">
            {[
              { id: "a1", type: "file" as const, mediaType: "application/pdf", filename: "report-review.pdf", url: "" },
              { id: "a2", type: "file" as const, mediaType: "text/plain", filename: "athena-4641.diff", url: "" },
            ].map((a) => (
              <Attachment key={a.id} data={a} onRemove={() => {}}>
                <AttachmentPreview />
                <AttachmentInfo />
                <AttachmentRemove />
              </Attachment>
            ))}
          </Attachments>
        </Specimen>
        <Specimen label="Conversation (empty state)" className="flex-col items-stretch">
          <Conversation className="h-56 rounded-2xl border border-border">
            <ConversationContent>
              <ConversationEmptyState icon={<Icons.ChatCircle size={22} weight="duotone" />} title="Ask Neko anything" description="It sees your tasks and asks before it changes anything." />
            </ConversationContent>
          </Conversation>
        </Specimen>
        <Specimen label="JsonViewer" className="flex-col items-stretch">
          <div className="rounded-xl border border-border p-3"><JsonViewer data={sampleArgs} rootName="arguments" defaultExpanded={open} /></div>
        </Specimen>
      </div>
    </Section>
  );
}

function WorkspaceSection() {
  const note = (text: string) => <p className="p-5 text-[13.5px] text-muted-foreground">{text}</p>;
  return (
    <Section id="workspace" eyebrow="Workspace" title={<>Tabs you can arrange.</>}>
      <div className="grid items-start gap-8 lg:grid-cols-[2fr_1fr]">
        <Specimen label="DockWorkspace: drag a tab to an edge to split, onto another pane to group, double-click to maximise" className="flex-col items-stretch">
          <div className="h-[420px] overflow-hidden rounded-2xl border border-border">
            <DockWorkspace
              storageKey="neko:dock:gallery"
              panels={[
                { id: "chat", title: "Chat", icon: Icons.ChatCircle, closable: false, render: () => note("The conversation lives here. Move this tab anywhere.") },
                { id: "changes", title: "Changes", icon: GitDiff, render: () => note("A diff of what the agent changed.") },
                { id: "plan", title: "Plan", icon: Icons.ListChecks, render: () => note("The plan the agent wrote.") },
                { id: "details", title: "Details", icon: Icons.Info, render: () => note("Status, repository and the activity log.") },
              ]}
              defaultLayout={[{ panels: ["chat"], size: 0.4 }, { panels: ["changes", "plan", "details"], of: 0, direction: "right" }]}
            />
          </div>
        </Specimen>
        <div className="flex flex-col gap-8">
          <Specimen label="ThemeMenu (one button, a menu)">
            <ThemeMenu />
          </Specimen>
        </div>
      </div>
    </Section>
  );
}

function SidebarSection() {
  return (
    <Section id="sidebar" eyebrow="Sidebar" title={<>Four ways to tidy the sidebar.</>}>
      <p className="max-w-2xl text-[13.5px] text-muted-foreground">
        Proposals drawn from sample data: click, fold and collapse each one to try it. The real sidebar is unchanged. Each keeps Neko&rsquo;s mark, ⌘K and the task rail, and takes its spacing from hushbacks&rsquo; sidebar.
      </p>
      <SidebarOptions />
    </Section>
  );
}

export default function Gallery() {
  return (
    <div data-slot="gallery" className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-16 px-8 py-10">
        <PageHeader
          eyebrow="Design system"
          title={<>UI gallery.</>}
          subtitle="Every primitive, editorial and chat component Neko's web UI is built from, with realistic data. Switch light and dark in the sidebar."
        />
        <Foundations />
        <Controls />
        <Forms />
        <Overlays />
        <DataDisplay />
        <Editorial />
        <ChatSection />
        <WorkspaceSection />
        <SidebarSection />
      </div>
    </div>
  );
}
