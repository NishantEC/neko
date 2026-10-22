// from hushbacks apps/spa/src/hooks/useOpenAfterMount.ts @5d9a64b11 (unchanged)
// A sheet mounted already open never plays its entry; open it one tick later.
import { useEffect, useState } from "react";

export function useOpenAfterMount() {
  const [open, setOpen] = useState(false);
  useEffect(() => setOpen(true), []);
  return [open, setOpen] as const;
}
