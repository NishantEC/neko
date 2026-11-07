// Neko's own viewers and reply fences, registered through the built-in host
// exactly as a plugin registers its own (canvas/viewers.ts). Keep the kinds in
// step with `neko_core::viewers::BUILTIN`, which is what the model is told.
//
// Light viewers (page, image, pdf, document) are here; the heavy ones are
// chunks of their own, loaded the first time an artifact of their kind opens:
// 3D (three.js), the sketch (Excalidraw), code and tables.
import { useEffect, useRef, useState } from "react";
import { builtinHost } from "@/host";
import { cn } from "@/ui/lib/utils";
import ChatMarkdown from "@/chat/t3/ChatMarkdown";
import { readText, type ViewerProps } from "@/canvas/viewers";

/** A viewer that could not read its file says so, in place. */
export function ViewerProblem({ message }: { message: string }) {
  return <p role="alert" data-slot="viewer-problem" className="p-4 text-sm text-destructive">{message}</p>;
}

/** The text a viewer shows, read once per file. */
export function useViewerText(props: Pick<ViewerProps, "url" | "text">) {
  const [state, setState] = useState<{ text: string | null; error: string | null }>({ text: props.text ?? null, error: null });
  useEffect(() => {
    let live = true;
    setState({ text: props.text ?? null, error: null });
    if (typeof props.text === "string") return;
    readText(props).then(
      (text) => { if (live) setState({ text, error: null }); },
      (e) => { if (live) setState({ text: null, error: e instanceof Error ? e.message : String(e) }); },
    );
    return () => { live = false; };
  }, [props.url, props.text]); // eslint-disable-line react-hooks/exhaustive-deps
  return state;
}

/**
 * A page (html) or an svg, live in `<iframe sandbox="allow-scripts">` (never
 * `allow-same-origin`), served under the artifact's own CSP. In comment mode
 * the daemon's bridge (html only) outlines what the pointer is over and posts
 * the pick; only messages from this very frame are read.
 */
function PageViewer({ artifact, version, url, commenting = false, onPick, onCancel }: ViewerProps) {
  const frame = useRef<HTMLIFrameElement>(null);
  const handlers = useRef({ onPick, onCancel });
  handlers.current = { onPick, onCancel };
  const mode = useRef(commenting);
  mode.current = commenting;
  // The frame has an opaque origin, so a message to it can only be addressed to "*".
  const post = () => frame.current?.contentWindow?.postMessage({ type: "neko-comment-mode", on: mode.current }, "*");
  useEffect(post, [commenting, url]);
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const data = event.data;
      if (data?.type === "neko-pick" && typeof data.selector === "string" && data.selector) {
        handlers.current.onPick?.({ selector: data.selector.slice(0, 300), text: typeof data.text === "string" ? data.text.slice(0, 120) : "" });
      } else if (data?.type === "neko-comment-cancel") {
        handlers.current.onCancel?.();
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);
  if (!url) return <ViewerProblem message="A page is shown from a saved artifact only." />;
  return (
    <iframe
      ref={frame}
      key={url}
      src={url}
      title={`${artifact?.title ?? "Artifact"}, version ${version}`}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      onLoad={post}
      data-slot="artifact-frame"
      data-commenting={commenting || undefined}
      className={cn("block h-full w-full border-0 bg-white", commenting && "ring-2 ring-inset ring-primary")}
    />
  );
}

function ImageViewer(props: ViewerProps) {
  const { artifact, version, url, kind } = props;
  // svg goes in a sandboxed frame like a page (as it always has), which keeps
  // its animations and links as drawn.
  if (kind === "svg") return <PageViewer {...props} commenting={false} />;
  return (
    <div data-slot="image-viewer" className="flex h-full w-full items-center justify-center overflow-auto bg-muted/40 p-4">
      {url ? <img src={url} alt={`${artifact?.title ?? "Image"}, version ${version}`} className="max-h-full max-w-full rounded-md shadow-sm" /> : null}
    </div>
  );
}

/** The browser's own PDF viewer. The file is served under a policy that runs nothing else (web.rs `PDF_CSP`). */
function PdfViewer({ artifact, version, url }: ViewerProps) {
  if (!url) return <ViewerProblem message="A PDF is shown from a saved artifact only." />;
  return <iframe src={url} title={`${artifact?.title ?? "PDF"}, version ${version}`} referrerPolicy="no-referrer" data-slot="pdf-viewer" className="block h-full w-full border-0 bg-white" />;
}

/** Markdown, rendered as a reply is (no raw html), read only. */
function MarkdownViewer(props: ViewerProps) {
  const { text, error } = useViewerText(props);
  if (error) return <ViewerProblem message={error} />;
  return (
    <div data-slot="markdown-viewer" className={cn("h-full overflow-auto", props.inline ? "" : "px-6 py-5")}>
      <div className="mx-auto max-w-3xl">
        {text === null ? <p className="text-sm text-muted-foreground" aria-busy="true">Opening…</p> : <ChatMarkdown text={text} />}
      </div>
    </div>
  );
}

const CODE_KINDS = ["json", "txt", "log", "xml", "yaml", "toml", "ini", "js", "mjs", "ts", "tsx", "jsx", "css", "py", "rb", "rs", "go", "java", "kt", "swift", "c", "h", "cpp", "sh", "sql", "diff", "patch"];

let registered = false;

/** Registers the built-ins once (pages/index.ts imports this before any plugin loads). */
export function registerBuiltinViewers() {
  if (registered) return;
  registered = true;
  const host = builtinHost;
  host.registerViewer({ id: "html", title: "Page", kinds: ["html"], component: PageViewer });
  host.registerViewer({ id: "image", title: "Image", kinds: ["png", "jpg", "gif", "webp", "avif", "bmp", "ico", "svg"], component: ImageViewer });
  host.registerViewer({ id: "pdf", title: "PDF", kinds: ["pdf"], component: PdfViewer });
  host.registerViewer({ id: "markdown", title: "Document", kinds: ["md"], component: MarkdownViewer });
  host.registerViewer({ id: "csv", title: "Table", kinds: ["csv", "tsv"], load: () => import("@/canvas/viewers/CsvViewer") });
  host.registerViewer({ id: "code", title: "Code", kinds: CODE_KINDS, load: () => import("@/canvas/viewers/CodeViewer") });
  host.registerViewer({ id: "3d", title: "3D model", kinds: ["glb", "gltf", "stl", "obj"], load: () => import("@/canvas/viewers/ModelViewer") });
  host.registerViewer({ id: "excalidraw", title: "Sketch", kinds: ["excalidraw"], edits: true, load: () => import("@/canvas/viewers/SketchViewer") });
  // ```csv / ```tsv: the block's own text as a table. ```3d: an artifact id, drawn in the reply.
  // ```mermaid is not drawn: the library is ~30 MB of chunks (stubs/mermaid.ts), so it stays code.
  host.registerFence({ language: "csv", viewer: "csv" });
  host.registerFence({ language: "tsv", viewer: "csv" });
  host.registerFence({ language: "3d", viewer: "3d" });
}
