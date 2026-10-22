// The composer, after hushbacks' ChatInput (packages/ui/src/components/ChatInput).
//
// Kept: one rounded container with the controls inside it, auto-growing
// textarea, image paste / drop / attach, the send ↔ stop morph, character count
// near the limit. Dropped: GCS upload, speech-to-text, the effort menu. Added:
// a `tray` slot that docks queued messages / decisions onto the top edge, and a
// `leading` slot for the "Neko decides" chip.
//
// Images stay local (data URLs): the daemon's CSP allows `data:` but not `blob:`
// images, and nothing uploads. A page that wants to send them reads
// `attachments`.
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { ArrowUp, CircleNotch, Image as ImageIcon, Square, X } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import { pickPastedImage } from "@/chat/clipboard";

export interface ChatAttachment {
  id: string;
  name: string;
  type: string;
  /** A data: URL, so it renders under the daemon's CSP and survives a re-render. */
  previewUrl: string;
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
  disabled?: boolean;
  sending?: boolean;
  /** Docked onto the top edge of the composer: queued messages, a decision. */
  tray?: React.ReactNode;
  /** Left of the toolbar: the chip that says who decides the model. */
  leading?: React.ReactNode;
  /** Shown above the composer in the danger colour. */
  error?: string | null;
  onKeyDown?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  allowImages?: boolean;
  attachments?: ChatAttachment[];
  onAttachmentsChange?: (next: ChatAttachment[]) => void;
  className?: string;
}

export function ChatInput({
  ref, text, onTextChange, onSendMessage, isGenerating = false, onStop,
  placeholder = "Ask Neko anything", maxLength = 4096, disabled = false, sending = false,
  tray, leading, error, onKeyDown, allowImages = false, attachments: controlled, onAttachmentsChange, className,
}: ChatInputProps) {
  const inner = useRef<HTMLTextAreaElement | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const [internal, setInternal] = useState<ChatAttachment[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const attachments = controlled ?? internal;

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
        setAttachments([...attachments, { id: crypto.randomUUID(), name: file.name, type: file.type, previewUrl }]);
      } catch (e) {
        setProblem(e instanceof Error ? e.message : String(e));
      }
    },
    [attachments, setAttachments],
  );

  const trimmed = text.trim();
  const tooLong = trimmed.length > maxLength;
  const hasContent = Boolean(trimmed || attachments.length);
  const showStop = isGenerating && !hasContent;
  const nearLimit = text.length >= maxLength - 200;
  const message = error ?? problem;

  return (
    <div className={cn("relative", className)}>
      {tray}
      {message ? <p role="alert" className="mb-2 ms-1.5 text-[13px] text-destructive">{message}</p> : null}
      <form
        data-slot="composer"
        data-tray={Boolean(tray)}
        onSubmit={(e) => {
          e.preventDefault();
          if (hasContent && !disabled && !sending && !tooLong) onSendMessage();
        }}
        onPaste={(e) => {
          if (!allowImages) return;
          const file = pickPastedImage(e.clipboardData);
          if (!file) return;
          e.preventDefault();
          void addImage(file);
        }}
        onDragOver={(e) => {
          if (!allowImages || !Array.from(e.dataTransfer.types).includes("Files")) return;
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          if (!allowImages) return;
          e.preventDefault();
          setDragging(false);
          const file = e.dataTransfer.files[0];
          if (file) void addImage(file);
        }}
        className={cn(
          "relative z-[1] flex flex-col gap-1 rounded-3xl border border-border bg-card p-2 shadow-xs transition-[border-color,box-shadow] duration-150",
          "focus-within:border-foreground/20 focus-within:ring-[3px] focus-within:ring-ring/10",
          dragging && "border-primary ring-[3px] ring-primary/20",
        )}
      >
        {attachments.length > 0 ? (
          <ul className="flex flex-wrap gap-2 px-2 pt-1.5" aria-label="Attached images">
            {attachments.map((a) => (
              <li key={a.id} className="group relative">
                <img src={a.previewUrl} alt={a.name || "Pasted image"} className="size-14 rounded-xl object-cover ring-1 ring-border" />
                <button
                  type="button"
                  aria-label={`Remove ${a.name || "image"}`}
                  onClick={() => setAttachments(attachments.filter((x) => x.id !== a.id))}
                  className="absolute -right-1.5 -top-1.5 grid size-5 place-items-center rounded-full bg-foreground text-background ring-2 ring-card transition-colors hover:bg-destructive hover:text-white"
                >
                  <X size={10} weight="bold" />
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
          onChange={(e) => onTextChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) e.preventDefault();
            onKeyDown?.(e);
          }}
          className="block max-h-[220px] min-h-6 w-full resize-none overflow-y-auto border-0 bg-transparent px-3 pt-2 pb-1 text-[14px] leading-[1.625] text-strong outline-none placeholder:text-muted-foreground/70 disabled:opacity-60"
        />
        <div className="flex items-center gap-2 px-1 pb-0.5">
          {allowImages ? (
            <>
              <input
                ref={fileInput}
                type="file"
                accept={IMAGE_TYPES.join(",")}
                className="hidden"
                aria-label="Attach image"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  e.target.value = "";
                  if (file) void addImage(file);
                }}
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="button" variant="ghost" size="icon-sm" className="rounded-full text-muted-foreground" onClick={() => fileInput.current?.click()}>
                    <ImageIcon />
                    <span className="sr-only">Attach image</span>
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">Attach image</TooltipContent>
              </Tooltip>
            </>
          ) : null}
          {leading}
          <span className="flex-1" />
          {nearLimit ? (
            <span aria-live="polite" className={cn("text-xs tabular-nums", tooLong ? "text-destructive" : "text-muted-foreground")}>
              {trimmed.length.toLocaleString()} / {maxLength.toLocaleString()}
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
              aria-label={isGenerating ? "Queue message" : "Send"}
              disabled={!hasContent || disabled || sending || tooLong}
              className="shrink-0 rounded-full"
            >
              {sending ? <CircleNotch className="animate-spin" /> : <ArrowUp weight="bold" />}
            </Button>
          )}
        </div>
      </form>
    </div>
  );
}
