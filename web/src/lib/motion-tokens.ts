// from hushbacks apps/spa/src/lib/motion-tokens.ts @5d9a64b11 (unchanged)
type MotionDuration = "quick" | "base" | "gentle" | "emphasis";
type MotionDistance = "sm" | "base" | "lg";

function readToken(name: string) {
  if (typeof document === "undefined") return "";
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function motionMs(duration: MotionDuration) {
  return Number.parseFloat(readToken(`--motion-${duration}`)) || 0;
}

export function motionPx(distance: MotionDistance) {
  const name = distance === "base" ? "--motion-distance" : `--motion-distance-${distance}`;
  return Number.parseFloat(readToken(name)) || 0;
}

export function motionEaseOut() {
  return readToken("--ease-out") || "cubic-bezier(0.22, 1, 0.36, 1)";
}

export function prefersReducedMotion() {
  if (typeof window === "undefined" || !window.matchMedia) return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
