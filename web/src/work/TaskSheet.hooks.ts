// from hushbacks apps/spa/src/pages/Builder/components/ResponseDetailPanel.hooks.ts @5d9a64b11
// The quick fade the sheet's content plays when it moves to another task (not on first open).
import { useState } from "react";

export function useSwapFade(id: string) {
  const [shown, setShown] = useState({ id, swapped: false });
  if (shown.id !== id) setShown({ id, swapped: true });
  return shown.swapped ? "sheet-swap" : undefined;
}
