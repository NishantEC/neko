export type Appearance = "light" | "dark";
export interface Glow { color: string; alpha: number; x: number; y: number; width: number; height: number }
export interface Backdrop { blur: number; base: string[]; glows?: Glow[] }
export interface ThemeData {
  id: string;
  name: string;
  appearance: Appearance;
  family?: string;
  colors: Partial<Record<string, string>> & { background: string; foreground: string; primary: string };
  backdrop?: Backdrop;
}
export const COLOR_KEYS: string[];
export const REQUIRED_COLORS: string[];
export const TOP_KEYS: string[];
export const BACKDROP_ALPHA: Record<string, number>;
export function parseHex(text: unknown): [number, number, number, number] | null;
export function toHex(rgb: number[]): string;
export function rgba(color: string | number[], alpha: number): string;
export function mix(from: string, to: string, t: number): string;
export function luminance(color: string): number;
export function over(color: string, base: string): string;
export function contrast(a: string, b: string): number;
export function ensure(color: string, surfaces: string[], min?: number): string;
export function effectiveSurfaces(theme: ThemeData, solid: Record<string, string>): Record<string, string>;
export function resolveColors(theme: ThemeData): Record<string, any>;
export function deriveVars(theme: ThemeData): [string, string][];
export function themeRule(theme: ThemeData, options?: { user?: boolean }): string;
export function themesCss(themes: ThemeData[], options?: { user?: boolean }): string;
export function contrastPairs(theme: ThemeData): [string, string, string][];
export function contrastFailures(theme: ThemeData, min?: number): { label: string; ratio: number }[];
export function structureFailures(theme: ThemeData): { label: string; ratio: number }[];
export function validateTheme(value: unknown, expectedId?: string): string | null;
