// Imported before Excalidraw itself (SketchViewer.tsx).
//
// 1. `EXCALIDRAW_ASSET_PATH`: Neko serves Excalidraw's fonts at
//    /excalidraw/fonts/ (vite.config.ts copies them), so every font's first
//    source is this site.
// 2. Excalidraw always adds esm.sh as a second source for every font face.
//    The page policy refuses it (fonts only from 'self'), and Chrome reports a
//    violation for each refused source as soon as the face is added, used or
//    not: 230 console errors for one sketch. Font faces are created through
//    this wrapper instead, which drops sources the policy would refuse anyway
//    (another origin) and the ones Neko does not ship (the 12 MB Xiaolai CJK
//    font; such a face falls back to `local()`, which never fetches). Every
//    other FontFace on the page is untouched.
declare global {
  interface Window { EXCALIDRAW_ASSET_PATH?: string | string[] }
}
window.EXCALIDRAW_ASSET_PATH = "/excalidraw/";

const NOT_SHIPPED = /\/fonts\/Xiaolai\//;

function sameOriginSources(src: string): string {
  const parts = src.split(/,\s*(?=url\(|local\()/);
  const kept = parts.filter((part) => {
    const url = /^url\(\s*['"]?([^'")]+)['"]?\s*\)/.exec(part.trim())?.[1];
    if (!url) return true;
    try {
      return new URL(url, location.href).origin === location.origin && !NOT_SHIPPED.test(url);
    } catch {
      return false;
    }
  });
  return kept.join(", ");
}

const Native = window.FontFace;
if (Native && !(Native as unknown as { nekoSameOrigin?: boolean }).nekoSameOrigin) {
  const Wrapped = function FontFace(family: string, source: string | BufferSource, descriptors?: FontFaceDescriptors) {
    const src = typeof source === "string" && source.includes("url(") ? sameOriginSources(source) || `local("${family.replace(/"/g, "")}")` : source;
    return new Native(family, src, descriptors);
  } as unknown as typeof FontFace;
  Wrapped.prototype = Native.prototype;
  (Wrapped as unknown as { nekoSameOrigin: boolean }).nekoSameOrigin = true;
  window.FontFace = Wrapped;
}

export {};
