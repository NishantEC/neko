// A handful of 16px line icons, drawn inline so the app needs no icon package.
import type { SVGProps } from "react";

const base = (props: SVGProps<SVGSVGElement>) => ({
  width: 16, height: 16, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor",
  strokeWidth: 1.4, strokeLinecap: "round" as const, strokeLinejoin: "round" as const,
  "aria-hidden": true, ...props,
});

export const Chevron = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="m4 6 4 4 4-4" /></svg>
);
export const Tools = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M9.5 3.2a3 3 0 0 0 3.3 3.9L7 12.9a1.4 1.4 0 0 1-2-2l5.8-5.8" /></svg>
);
export const Terminal = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="m3 5 3 3-3 3M8 12h5" /></svg>
);
export const Copy = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><rect x="5.5" y="5.5" width="7" height="7" rx="1.5" /><path d="M10.5 3.5v-.2A.8.8 0 0 0 9.700 2.500H3.800a.8.8 0 0 0-.8.8v5.900a.8.8 0 0 0 .8.8h.2" /></svg>
);
export const ArrowUp = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} strokeWidth={1.8}><path d="M8 13V3M3.500 7.500 8 3l4.500 4.500" /></svg>
);
export const Square = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="currentColor" stroke="none"><rect x="4" y="4" width="8" height="8" rx="1.500" /></svg>
);
export const Home = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M3 7.500 8 3l5 4.500V13H3z" /></svg>
);
export const Board = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><rect x="2.500" y="3" width="3" height="10" rx="1" /><rect x="6.500" y="3" width="3" height="6" rx="1" /><rect x="10.500" y="3" width="3" height="8" rx="1" /></svg>
);
export const Puzzle = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M6 3h2v1.500a1 1 0 1 0 2 0V3h3v3h-1.500a1 1 0 1 0 0 2H13v5H3V9h1.500a1 1 0 1 0 0-2H3V4z" /></svg>
);
export const X = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="m4 4 8 8M12 4l-8 8" /></svg>
);
export const Pencil = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="m10.500 3.500 2 2L6 12l-2.800.8L4 10z" /></svg>
);
export const Steer = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M3.500 12V9a2 2 0 0 1 2-2h7M10 4l3 3-3 3" /></svg>
);
export const Shield = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M8 2 3.500 3.800V8c0 2.700 1.900 4.500 4.500 6 2.600-1.500 4.500-3.300 4.500-6V3.800z" /></svg>
);
