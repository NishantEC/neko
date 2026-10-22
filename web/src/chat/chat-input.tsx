// The composer, after hushbacks' ChatInput (packages/ui/src/components/ChatInput)
// and Neko's native composer (SharedComposer.swift).
//
// Kept: one rounded container with the controls inside it, auto-growing
// textarea, paste / drop / choose attachments, the send ↔ queue ↔ stop morph,
// character count near the limit. Dropped: GCS upload, speech-to-text.
// Added: a `tray` slot that docks queued messages / decisions onto the top
// edge, `leading` / `controls` slots for the toolbar (mode switch, runtime
// picker), a `menu` slot that floats ABOVE the composer for `/` and `@`, and
// attachments of any file kind that a page uploads (`onFiles`).
//
// Images keep a local data: URL preview (the daemon's CSP allows `data:` but
// not `blob:` images). A page that uploads reads `attachments`.
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { ArrowUp, CircleNotch, File as FileIcon, Image as ImageIcon, Paperclip, Square, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import { pickPastedImage } from "@/chat/clipboard";
import { byteLength } from "@/chat/ChatInputSlash";

export interface ChatAttachment {
  id: string;
  name: string;
  type: string;
  /** A data: URL, so it renders under the daemon's CSP and survives a re-render. */
  previewUrl?: string;
  isImage?: boolean;
  status?: "uploading" | "ready" | "error";
  error?: string;
}

const IMAGE_TYPES = ["image/jpeg", "image/png", "image/webp", "image/gif"];
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

function readDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("The image could not be read."));
    reader.readAsDataURL(file);
  });
}

/** Files on the clipboard, unless it also carries text (then it is a text paste). */
function pastedFiles(data: DataTransfer): File[] {
  if (data.getData("text/plain").trim()) return [];
  return Array.from(data.files).map((file) =>
    file.name ? file : new File([file], `pasted-image.${file.type.split("/")[1] || "png"}`, { type: file.type }),
  );
}

export interface ChatInputProps {
  ref?: React.Ref<HTMLTextAreaElement>;
  text: string;
  onTextChange: (text: string) => void;
  onSendMessage: () => void;
  /** A reply is running: an empty composer shows the stop square, a typed one queues. */
  isGenerating?: boolean;
  onStop?: () => void;
  placeholder?: string;
  maxLength?: number;
  /** Count and limit the message in UTF-8 bytes of `sizeText` (default `text`), as the daemon does. */
  limitBytes?: number;
  sizeText?: string;
  disabled?: boolean;
  sending?: boolean;
  /** Docked onto the top edge of the composer: queued messages, a decision. */
  tray?: React.ReactNode;
  /** Floats above the composer (the `/` and `@` menu). Nothing in the page moves when it opens. */
  menu?: React.ReactNode;
  /** The combobox wiring for the textarea while `menu` is open. */
  menuState?: { id: string; activeId?: string } | null;
  /** Left of the toolbar, after the attach button: the Ask / Plan switch. */
  leading?: React.ReactNode;
  /** Toolbar controls beside `leading`: the runtime picker. */
  controls?: React.ReactNode;
  /** Shown above the composer in the danger colour. */
  error?: string | null;
  onKeyDown?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  /** Images only, kept in the browser (the original behaviour). */
  allowImages?: boolean;
  /** Files of any kind: paste, drop or choose; the page uploads them. */
  onFiles?: (files: File[]) => void;
  attachments?: ChatAttachment[];
  onAttachmentsChange?: (next: ChatAttachment[]) => void;
  onRemoveAttachment?: (id: string) => void;
  /** Label of the send button's primary action; "Queue message" while a reply runs. */
  className?: string;
}

export function ChatInput({
  ref, text, onTextChange, onSendMessage, isGenerating = false, onStop,
  placeholder = "Ask Neko anything", maxLength = 4096, limitBytes, sizeText, disabled = false, sending = false,
  tray, menu, menuState, leading, controls, error, onKeyDown, allowImages = false, onFiles, attachments: controlled,
  onAttachmentsChange, onRemoveAttachment, className,
}: ChatInputProps) {
  const inner = useRef<HTMLTextAreaElement | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const [internal, setInternal] = useState<ChatAttachment[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const attachments = controlled ?? internal;
  const accepts = Boolean(allowImages || onFiles);

  const setAttachments = useCallback(
    (next: ChatAttachment[]) => {
      if (controlled === undefined) setInternal(next);
      onAttachmentsChange?.(next);
    },
    [controlled, onAttachmentsChange],
  );

  const setRefs = useCallback(
    (el: HTMLTextAreaElement | null) => {
      inner.current = el;
      if (typeof ref === "function") ref(el);
      else if (ref) (ref as React.RefObject<HTMLTextAreaElement | null>).current = el;
    },
    [ref],
  );

  // Auto-grow to the content, up to 220px, then scroll.
  useLayoutEffect(() => {
    const el = inner.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [text]);

  const addImage = useCallback(
    async (file: File) => {
      if (!IMAGE_TYPES.includes(file.type)) return setProblem("Only images are supported (JPEG, PNG, WebP, GIF).");
      if (file.size > MAX_IMAGE_BYTES) return setProblem("That image is over 10 MB.");
      setProblem(null);
      try {
        const previewUrl = await readDataUrl(file);
        setAttachments([...attachments, { id: crypto.randomUUID(), name: file.name, type: file.type, previewUrl, isImage: true }]);
      } catch (e) {
        setProblem(e instanceof Error ? e.message : String(e));
      }
    },
    [attachments, setAttachments],
  );

  const take = useCallback(
    (files: File[]) => {
      if (!files.length) return;
      setProblem(null);
      if (onFiles) onFiles(files);
      else if (allowImages) void addImage(files[0]);
    },
    [onFiles, allowImages, addImage],
  );

  const trimmed = text.trim();
  const sizeUsed = limitBytes ? byteLength(sizeText ?? text) : trimmed.length;
  const limit = limitBytes ?? maxLength;
  const tooLong = sizeUsed > limit;
  const uploading = attachments.some((a) => a.status === "uploading");
  const hasContent = Boolean(trimmed || attachments.some((a) => a.status !== "error"));
  const showStop = isGenerating && !hasContent;
  const nearLimit = sizeUsed >= limit - Math.min(200, limit / 5);
  const message = error ?? problem;
  const canSend = hasContent && !disabled && !sending && !tooLong && !uploading;

  return (
    <div className={cn("relative", className)}>
      {menu}
      {tray}
      {message ? <p role="alert" data-slot="composer-error" className="mb-2 ms-1.5 text-[13px] text-destructive">{message}</p> : null}
      <form
        data-slot="composer"
        data-tray={Boolean(tray)}
        onSubmit={(e) => {
          e.preventDefault();
          if (canSend) onSendMessage();
        }}
        onPaste={(e) => {
          if (!accepts) return;
          const files = onFiles ? pastedFiles(e.clipboardData) : [pickPastedImage(e.clipboardData)].filter((f): f is File => !!f);
          if (!files.length) return;
          e.preventDefault();
          take(files);
        }}
        onDragOver={(e) => {
          if (!accepts || !Array.from(e.dataTransfer.types).includes("Files")) return;
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          if (!accepts) return;
          e.preventDefault();
          setDragging(false);
          take(Array.from(e.dataTransfer.files));
        }}
        className={cn(
          "relative z-[1] flex flex-col gap-1 rounded-3xl border border-border bg-card p-2 shadow-xs transition-[border-color,box-shadow] duration-150",
          "focus-within:border-foreground/20 focus-within:ring-[3px] focus-within:ring-ring/10",
          dragging && "border-primary ring-[3px] ring-primary/20",
        )}
      >
        {attachments.length > 0 ? (
          <ul data-slot="attachments" className="flex flex-wrap gap-2 px-2 pt-1.5" aria-label="Attachments">
            {attachments.map((a) => (
              <li
                key={a.id}
                data-slot="attachment"
                data-status={a.status ?? "ready"}
                className={cn("flex max-w-[240px] items-center gap-2 rounded-xl bg-foreground/[0.05] py-1.5 ps-2 pe-1", a.status === "error" && "ring-1 ring-destructive/50")}
              >
                {a.isImage && a.previewUrl ? (
                  <img src={a.previewUrl} alt={a.name || "Pasted image"} className="size-8 shrink-0 rounded-lg object-cover ring-1 ring-border" />
                ) : (
                  <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-foreground/[0.06] text-muted-foreground">
                    {a.isImage ? <ImageIcon /> : <FileIcon />}
                  </span>
                )}
                <span className="min-w-0 leading-tight">
                  <span className="block truncate text-[12.5px] font-medium text-strong" title={a.name}>{a.name}</span>
                  <span className={cn("block truncate text-[11px]", a.status === "error" ? "text-destructive" : "text-muted-foreground")} title={a.error}>
                    {a.status === "uploading" ? "Uploading…" : a.status === "error" ? (a.error ?? "Could not attach") : a.isImage ? "Image" : "File"}
                  </span>
                </span>
                <button
                  type="button"
                  aria-label={`Remove ${a.name || "attachment"}`}
                  onClick={() => (onRemoveAttachment ? onRemoveAttachment(a.id) : setAttachments(attachments.filter((x) => x.id !== a.id)))}
                  className="grid size-6 shrink-0 place-items-center rounded-full text-muted-foreground outline-none transition-colors hover:bg-foreground/10 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <X size={11} weight="bold" />
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        <textarea
          ref={setRefs}
          rows={1}
          value={text}
          disabled={disabled}
          placeholder={placeholder}
          aria-label="Message Neko"
          role={menuState ? "combobox" : undefined}
          aria-expanded={menuState ? true : undefined}
          aria-controls={menuState?.id}
          aria-activedescendant={menuState?.activeId}
          aria-autocomplete={menuState ? "list" : undefined}
          onChange={(e) => onTextChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) e.preventDefault();
            onKeyDown?.(e);
          }}
          className="block max-h-[220px] min-h-6 w-full resize-none overflow-y-auto border-0 bg-transparent px-3 pt-2 pb-1 text-[14px] leading-[1.625] text-strong outline-none placeholder:text-muted-foreground/70 disabled:opacity-60"
        />
        <div className="flex items-center gap-2 px-1 pb-0.5">
          {accepts ? (
            <>
              <input
                ref={fileInput}
                type="file"
                multiple={Boolean(onFiles)}
                accept={onFiles ? undefined : IMAGE_TYPES.join(",")}
                className="hidden"
                tabIndex={-1}
                aria-label="Choose files to attach"
                onChange={(e) => {
                  const files = Array.from(e.target.files ?? []);
                  e.target.value = "";
                  take(files);
                }}
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="button" variant="ghost" size="icon-sm" className="rounded-full text-muted-foreground" onClick={() => fileInput.current?.click()}>
                    <Paperclip />
                    <span className="sr-only">Attach files</span>
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">Attach images or files; you can also paste or drop them</TooltipContent>
              </Tooltip>
            </>
          ) : null}
          {leading}
          {controls}
          <span className="flex-1" />
          {nearLimit ? (
            <span aria-live="polite" className={cn("text-xs tabular-nums", tooLong ? "text-destructive" : "text-muted-foreground")}>
              {sizeUsed.toLocaleString()} / {limit.toLocaleString()}
            </span>
          ) : null}
          {showStop ? (
            <Button
              type="button"
              data-slot="send"
              data-mode="stop"
              size="icon"
              aria-label="Stop"
              title="Stop this reply"
              className="shrink-0 rounded-full border-destructive bg-destructive text-white hover:bg-destructive/90"
              onClick={onStop}
            >
              <Square weight="fill" className="size-3.5" />
            </Button>
          ) : (
            <Button
              type="submit"
              data-slot="send"
              data-mode={isGenerating ? "queue" : "send"}
              size="icon"
              aria-label={sending ? "Sending" : isGenerating ? "Queue message" : "Send"}
              title={isGenerating ? "Queue message (⌘↵ interrupts and sends)" : "Send (↵)"}
              disabled={!canSend}
              className="shrink-0 rounded-full"
            >
              {sending || uploading ? <CircleNotch className="animate-spin" /> : <ArrowUp weight="bold" />}
            </Button>
          )}
        </div>
      </form>
    </div>
  );
}
