// Neko's own viewers and reply fences, registered through the built-in host
// exactly as a plugin registers its own (canvas/viewers.ts). Keep the kinds in
// step with `neko_core::viewers::BUILTIN`, which is what the model is told.
//
// Light viewers (page, image, pdf, document) are here; the heavy ones are
// chunks of their own, loaded the first time an artifact of their kind opens:
// 3D (three.js), the sketch (Excalidraw), code and tables.
import { useContext, useEffect, useRef, useState } from "react";
import { builtinHost } from "@/host";
import { cn } from "@/ui/lib/utils";
import ChatMarkdown from "@/chat/t3/ChatMarkdown";
import { readText, ViewerReport, type ViewerProps } from "@/canvas/viewers";
import { MAX_SEND, offerFromArtifact, reportProblem } from "@/canvas/artifacts";

/** A viewer that could not read its file says so, in place, and the agent that made the artifact is told (`Artifact { id }` status). */
export function ViewerProblem({ message }: { message: string }) {
  const report = useContext(ViewerReport);
  useEffect(() => { report?.(message); }, [report, message]);
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

const PROBLEM_KINDS = ["error", "rejection", "console"] as const;
/** Problems one load of a page may report, whatever its scripts post. */
const PROBLEMS_PER_LOAD = 25;

/** Whether the person has just interacted with this window or a frame in it (user activation passes up to the parent). */
const personActed = () => {
  const activation = (navigator as Navigator & { userActivation?: { isActive: boolean } }).userActivation;
  return activation ? activation.isActive : true;
};

/**
 * A page (html) or an svg, live in `<iframe sandbox="allow-scripts">` (never
 * `allow-same-origin`), served under the artifact's own CSP. Only messages
 * whose source is this very frame are read; everything in them is checked
 * and bounded, since the page's own scripts could post them too:
 *
 * - Every message names the artifact and version the daemon served it as
 *   (`id`, `v`); one naming anything else (a frame that navigated) is dropped.
 * - `neko-pick` / `neko-comment-cancel`: only in comment mode (the daemon's
 *   bridge outlines what the pointer is over and posts the pick, with its
 *   stamped `data-neko-id` and opening tag).
 * - `neko-problem`: an uncaught error, unhandled rejection or console.error
 *   (the daemon's probe), passed on as `ReportArtifactProblem`.
 * - `neko-send`: a click on a `data-neko-send` element. Its text becomes a
 *   draft in Home's composer, never a sent message, and only while this frame
 *   has focus right after the person acted, one per 2 s, 5 waiting at most.
 *   (No web API lets the parent consume the frame's user activation, so the
 *   2 s gap stands in for "one draft per activation".)
 */
function PageViewer({ artifact, version, url, commenting = false, onPick, onCancel }: ViewerProps) {
  const frame = useRef<HTMLIFrameElement>(null);
  const handlers = useRef({ onPick, onCancel, artifact, version });
  handlers.current = { onPick, onCancel, artifact, version };
  const mode = useRef(commenting);
  mode.current = commenting;
  const problems = useRef(0);
  const lastOffer = useRef(0);
  // The frame has an opaque origin, so a message to it can only be addressed to "*".
  const post = () => frame.current?.contentWindow?.postMessage({ type: "neko-comment-mode", on: mode.current }, "*");
  useEffect(post, [commenting, url]);
  useEffect(() => { problems.current = 0; }, [url]);
  useEffect(() => {
    const text = (value: unknown, max: number) => (typeof value === "string" ? value.slice(0, max) : "");
    const onMessage = (event: MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow) return;
      const data = event.data;
      const { artifact: shown, version: n } = handlers.current;
      if (!data || typeof data !== "object") return;
      // The bridge names the version it was served as; anything else is not this version's.
      if (shown && (data.id !== shown.id || data.v !== n)) return;
      if (data.type === "neko-pick" && mode.current && typeof data.selector === "string" && data.selector) {
        const element = text(data.element, 64);
        handlers.current.onPick?.({ selector: data.selector.slice(0, 300), text: text(data.text, 120), id: /^[\w-]{1,64}$/.test(element) ? element : undefined, hint: text(data.hint, 200) || undefined });
      } else if (data.type === "neko-comment-cancel" && mode.current) {
        handlers.current.onCancel?.();
      } else if (data.type === "neko-problem" && PROBLEM_KINDS.includes(data.kind) && typeof data.message === "string") {
        if (!shown || problems.current >= PROBLEMS_PER_LOAD) return;
        problems.current += 1;
        reportProblem(shown.id, n, data.kind, data.message.slice(0, 500), "html");
      } else if (data.type === "neko-send" && typeof data.text === "string" && shown && !mode.current) {
        const now = Date.now();
        if (document.activeElement !== frame.current || !personActed() || now - lastOffer.current < 2000) return;
        lastOffer.current = now;
        offerFromArtifact(shown, n, data.text.slice(0, 4 * MAX_SEND));
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
