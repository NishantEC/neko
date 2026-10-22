// from hushbacks apps/spa/src/pages/Builder/components/useFusedTabIndicator.ts @da20dfaeb
import { useCallback, useLayoutEffect, useRef, useState } from "react";

export const FUSED_TAB_EASE: [number, number, number, number] = [0.22, 1, 0.36, 1];
export const FUSED_TAB_DURATION = 0.34;

export type FusedTabPosition = {
  left: number;
  width: number;
};

export function useFusedTabIndicator(value: string) {
  const [position, setPosition] = useState<FusedTabPosition | null>(null);
  const [instant, setInstant] = useState(false);
  const positionRef = useRef<FusedTabPosition | null>(null);
  const syncedValueRef = useRef<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef(new Map<string, HTMLElement>());
  const labelRefs = useRef(new Map<string, HTMLElement>());

  const sync = useCallback(() => {
    const activeElement = tabRefs.current.get(value);
    if (!activeElement) return;

    const restoreLabelWidths: Array<[HTMLElement, string]> = [];
    for (const [labelValue, labelElement] of labelRefs.current) {
      restoreLabelWidths.push([labelElement, labelElement.style.width]);
      labelElement.style.width = labelValue === value ? "auto" : "0px";
    }

    const settled = { left: activeElement.offsetLeft, width: activeElement.offsetWidth };

    for (const [labelElement, width] of restoreLabelWidths) {
      labelElement.style.width = width;
    }

    const previous = positionRef.current;
    if (previous && previous.left === settled.left && previous.width === settled.width) return;

    const switchedTab = syncedValueRef.current !== value;
    syncedValueRef.current = value;
    positionRef.current = settled;
    setInstant(!switchedTab);
    setPosition(settled);
  }, [value]);

  useLayoutEffect(() => {
    sync();

    const listElement = listRef.current;
    const activeElement = tabRefs.current.get(value);
    if (!listElement || !activeElement) return;

    const observer = new ResizeObserver(sync);
    observer.observe(listElement);
    observer.observe(activeElement);

    return () => observer.disconnect();
  }, [sync, value]);

  return { position, instant, listRef, tabRefs, labelRefs };
}
