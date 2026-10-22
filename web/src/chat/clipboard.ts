// from hushbacks packages/ui/src/components/ChatInput/clipboard.ts
export interface ClipboardImageItem {
  kind: string;
  type: string;
  getAsFile(): File | null;
}

export interface ClipboardImageSource {
  getData(format: string): string;
  items?: ArrayLike<ClipboardImageItem> | null;
  files?: ArrayLike<File> | null;
}

export function pickPastedImage(clipboardData: ClipboardImageSource | null | undefined): File | null {
  if (!clipboardData) return null;
  if (clipboardData.getData("text/plain").trim()) return null;

  const fromItems = Array.from(clipboardData.items ?? [])
    .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .find((file): file is File => file !== null);

  const file = fromItems ?? Array.from(clipboardData.files ?? []).find((f) => f.type.startsWith("image/"));
  if (!file) return null;
  if (file.name) return file;

  const extension = file.type.split("/")[1] || "png";
  return new File([file], `pasted-image.${extension}`, { type: file.type });
}
