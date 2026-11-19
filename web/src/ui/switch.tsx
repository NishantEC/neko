// from ui-kit packages/ui/src/components/switch.tsx
// Design system 2026-11 (4.4): the settings switch is 46 x 28 (padding 4, thumb 20, travel 18, 200 ms); `size="sm"` is the base 36 x 22 (thumb 18).
// Off is the well; on is the ink (the pebble's text colour), no shadow.
import { Switch as SwitchPrimitive } from "@base-ui/react/switch";
import type * as React from "react";

import { cn } from "@/ui/lib/utils";

function Switch({
  className,
  size = "default",
  ...props
}: React.ComponentProps<typeof SwitchPrimitive.Root> & {
  size?: "sm" | "default";
}) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      data-size={size}
      className={cn(
        "ui-switch ui-round sky-switch shrink-0 rounded-full border border-transparent data-[size=default]:h-7 data-[size=default]:w-[46px] data-[size=default]:p-1 data-[size=sm]:h-[22px] data-[size=sm]:w-9 data-[size=sm]:p-0.5 peer group/switch relative inline-flex items-center transition-[background-color,box-shadow] duration-200 motion-reduce:transition-none outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring aria-invalid:shadow-[0_0_0_2px_color-mix(in_oklab,var(--destructive)_55%,transparent)] after:absolute after:-inset-x-3 after:-inset-y-2 data-disabled:cursor-not-allowed data-disabled:opacity-50",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className="ui-switch-thumb ui-round sky-switch-thumb rounded-full group-data-[size=default]/switch:size-5 group-data-[size=sm]/switch:size-[18px] group-data-[size=default]/switch:data-checked:translate-x-[18px] group-data-[size=sm]/switch:data-checked:translate-x-3.5 data-unchecked:translate-x-0 pointer-events-none block ring-0 transition-[translate,background-color] duration-200 ease-[var(--ease-out)] motion-reduce:transition-none"
      />
    </SwitchPrimitive.Root>
  );
}

export { Switch };
