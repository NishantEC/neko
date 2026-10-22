// from hushbacks packages/ui/src/components/form-portal-context.ts
import { createContext, useContext } from "react";

/**
 * Container target for Radix-portaled content rendered inside a
 * FormThemeScope. Radix primitives (Dialog, Select, Popover, DropdownMenu,
 * Tooltip, HoverCard, Sheet, AlertDialog) portal to `document.body` by
 * default, which escapes the scope and inherits builder chrome.
 *
 * Providing a container inside the FormThemeScope keeps portaled content
 * within the light/dark island, so the theme + dark-variant rules apply.
 */
export const FormPortalContext = createContext<HTMLElement | null>(null);

export function useFormPortalContainer(): HTMLElement | undefined {
  const container = useContext(FormPortalContext);
  return container ?? undefined;
}
