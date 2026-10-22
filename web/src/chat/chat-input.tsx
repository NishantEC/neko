// The composer. Chrome from t3code apps/web/src/components/chat/ChatComposer.tsx
// and ChatView.tsx @3b95e7831 (MIT): ComposerSurface, the banner dock, the body
// with the media row and file list, the footer with controls and the primary
// action. T3's Tiptap prompt editor is not ported — a textarea carries its
// classes, so Neko keeps its key handling and drafts. Behaviour after
// hushbacks' ChatInput (packages/ui/src/components/ChatInput) and Neko's native
// composer (SharedComposer.swift).
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
import { File as FileIcon, Image as ImageIcon, Paperclip, X } from "lucide-react";
import { ComposerSurface } from "@/chat/t3/ComposerSurface";
import { ComposerBanner } from "@/chat/t3/ComposerBanner";
import { ComposerPrimaryActions } from "@/chat/t3/ComposerPrimaryActions";
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

  // Auto-grow to the content, up to 208px (max-h-52), then scroll.
  useLayoutEffect(() => {
    const el = inner.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(el.scrollHeight, 208)}px`;
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
  const nearLimit = sizeUsed >= limit - Math.min(200, limit / 5);
  const message = error ?? problem;
  const canSend = hasContent && !disabled && !sending && !tooLong && !uploading;

  const imageAttachments = attachments.filter((a) => a.isImage && a.previewUrl);
  const fileAttachments = attachments.filter((a) => !(a.isImage && a.previewUrl));
  const removeAttachment = (a: ChatAttachment) =>
    onRemoveAttachment ? onRemoveAttachment(a.id) : setAttachments(attachments.filter((x) => x.id !== a.id));
  const attachmentStatus = (a: ChatAttachment) =>
    a.status === "uploading" ? "Uploading…" : a.status === "error" ? (a.error ?? "Could not attach") : a.isImage ? "Image" : "File";

  // T3 Code's composer chrome (ChatView + ChatComposer): ComposerSurface.Shell /
  // Host / Main, the banner dock for what sits on the composer's top edge, the
  // body with the media row and file list over the prompt, and the footer with
  // controls on the left and attach + the primary action on the right.
  return (
    <div className={cn("relative", className)}>
      {menu}
      {message ? <p role="alert" data-slot="composer-error" className="mb-2 ms-1.5 text-xs text-destructive">{message}</p> : null}
      <ComposerSurface.Shell>
        <ComposerSurface.Host>
          <form
            data-slot="composer"
            data-chat-composer-form="true"
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
            className="mx-auto w-full min-w-0 max-w-(--chat-content-max-width)"
          >
            {tray ? (
              <ComposerBanner.Dock>
                <ComposerBanner.Column>{tray}</ComposerBanner.Column>
              </ComposerBanner.Dock>
            ) : null}
            <div className="relative">
              <ComposerSurface.Main>
                <div
                  data-chat-composer-surface="true"
                  className={cn(
                    "rounded-3xl transition-[background-color] duration-200",
                    dragging ? "bg-accent/45 ring-1 ring-inset ring-primary/70" : null,
                  )}
                >
                  <div data-chat-composer-body="true" className="relative px-3 pt-3.5 pb-2 sm:px-4 sm:pt-4">
                    {attachments.length > 0 ? (
                      <div data-slot="attachments" role="list" aria-label="Attachments">
                        {imageAttachments.length > 0 ? (
                          <div className="mb-3 flex max-w-full flex-wrap gap-2">
                            {imageAttachments.map((a) => (
                              <div
                                key={a.id}
                                role="listitem"
                                data-slot="attachment"
                                data-status={a.status ?? "ready"}
                                className={cn(
                                  "group/attachment relative h-16 w-16 shrink-0 overflow-hidden rounded-lg border border-border/80 bg-background",
                                  a.status === "error" && "border-destructive/60",
                                )}
                              >
                                <img src={a.previewUrl} alt={a.name || "Pasted image"} className="h-full w-full object-cover" />
                                <span className="sr-only">
                                  {a.name} · {attachmentStatus(a)}
                                </span>
                                {a.status === "uploading" || a.status === "error" ? (
                                  <span aria-hidden className="pointer-events-none absolute inset-x-0 bottom-0 bg-background/85 px-1 text-center text-3xs text-foreground">
                                    {attachmentStatus(a)}
                                  </span>
                                ) : null}
                                <Button
                                  variant="secondary"
                                  size="icon-xs"
                                  className="absolute top-1 right-1 size-5 rounded-full bg-background/85 backdrop-blur-sm"
                                  onClick={() => removeAttachment(a)}
                                  aria-label={`Remove ${a.name || "attachment"}`}
                                >
                                  <X className="size-3" />
                                </Button>
                              </div>
                            ))}
                          </div>
                        ) : null}
                        {fileAttachments.length > 0 ? (
                          <div className="mb-3 flex flex-col gap-1">
                            {fileAttachments.map((a) => (
                              <div
                                key={a.id}
                                role="listitem"
                                data-slot="attachment"
                                data-status={a.status ?? "ready"}
                                className="flex min-w-0 items-center gap-2 py-1 text-sm text-foreground"
                              >
                                {a.isImage ? (
                                  <ImageIcon className="size-4 shrink-0 text-secondary-label" aria-hidden />
                                ) : (
                                  <FileIcon className="size-4 shrink-0 text-secondary-label" aria-hidden />
                                )}
                                <span className="min-w-0 flex-1 truncate text-left" title={a.name}>
                                  {a.name}
                                </span>
                                <span className={cn("shrink-0 text-xs", a.status === "error" ? "text-destructive" : "text-secondary-label")} title={a.error}>
                                  {attachmentStatus(a)}
                                </span>
                                <Button
                                  variant="ghost"
                                  size="icon-xs"
                                  className="size-6 text-muted-foreground"
                                  onClick={() => removeAttachment(a)}
                                  aria-label={`Remove ${a.name || "attachment"}`}
                                >
                                  <X className="size-3.5" />
                                </Button>
                              </div>
                            ))}
                          </div>
                        ) : null}
                      </div>
                    ) : null}
                    <div className="relative flow-root text-sm">
                      <textarea
                        ref={setRefs}
                        rows={1}
                        value={text}
                        disabled={disabled}
                        placeholder={placeholder}
                        aria-label="Message Neko"
                        data-testid="composer-editor"
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
                        className="-m-1 block max-h-52 min-h-19.5 w-[calc(100%+0.5rem)] resize-none overflow-y-auto border-0 bg-transparent p-1 whitespace-pre-wrap wrap-break-word leading-relaxed text-foreground outline-none placeholder:text-placeholder/75 focus:outline-none disabled:opacity-60"
                      />
                    </div>
                  </div>

                  <div
                    data-chat-composer-footer="true"
                    className="flex min-w-0 flex-nowrap items-center justify-between gap-2 overflow-visible px-3 pb-3 sm:px-4 sm:pb-4"
                  >
                    <div
                      data-chat-composer-controls="left"
                      className="relative -m-1 -ms-3.5 flex min-w-0 flex-1 items-center gap-1 overflow-x-auto p-1 ps-3.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
                    >
                      {leading}
                      {controls}
                    </div>
                    <div data-chat-composer-actions="right" className="flex shrink-0 flex-nowrap items-center justify-end gap-2">
                      {nearLimit ? (
                        <span aria-live="polite" className={cn("text-xs tabular-nums", tooLong ? "text-destructive" : "text-muted-foreground")}>
                          {sizeUsed.toLocaleString()} / {limit.toLocaleString()}
                        </span>
                      ) : null}
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
                            <TooltipTrigger
                              render={
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon-sm"
                                  className="size-8 rounded-full text-muted-foreground hover:text-foreground"
                                  onPointerDown={(event) => event.preventDefault()}
                                  onClick={() => fileInput.current?.click()}
                                  aria-label="Attach files"
                                />
                              }
                            >
                              <Paperclip className="size-4" />
                            </TooltipTrigger>
                            <TooltipContent side="top">Attach images or files; you can also paste or drop them</TooltipContent>
                          </Tooltip>
                        </>
                      ) : null}
                      <ComposerPrimaryActions
                        isRunning={isGenerating}
                        canInterrupt={isGenerating && !!onStop}
                        hasSendableContent={hasContent}
                        isSendBusy={sending || uploading}
                        disabled={disabled || tooLong}
                        onInterrupt={onStop}
                      />
                    </div>
                  </div>
                </div>
              </ComposerSurface.Main>
            </div>
          </form>
        </ComposerSurface.Host>
      </ComposerSurface.Shell>
    </div>
  );
}
