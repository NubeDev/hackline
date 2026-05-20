import * as React from "react";
import * as Tooltip from "@radix-ui/react-tooltip";

import { cn } from "@/lib/utils";

// Help bubble shown next to a page title. Hover / focus / tap opens
// it; content is short prose intended to orient a new operator
// (what the page is, what to do here, what to check when it's
// empty or broken). Kept inline so a contributor adding a page
// can write the help text alongside the title.
export function HelpTip({
  children,
  className,
  align = "start",
  side = "bottom",
}: {
  children: React.ReactNode;
  className?: string;
  align?: "start" | "center" | "end";
  side?: "top" | "right" | "bottom" | "left";
}) {
  // Local provider so a page can render the tip without the root
  // having to set one up. delayDuration=0 makes the hover feel
  // immediate; the tooltip is the whole point of clicking the
  // icon, no need to make people wait.
  return (
    <Tooltip.Provider delayDuration={0} skipDelayDuration={0}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button
            type="button"
            aria-label="What is this page?"
            className={cn(
              "inline-flex h-5 w-5 items-center justify-center rounded-full border border-border bg-muted text-[11px] font-semibold text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              className,
            )}
          >
            ?
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            side={side}
            align={align}
            sideOffset={6}
            collisionPadding={12}
            className="z-50 max-w-sm rounded-md border bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground shadow-md"
          >
            {children}
            <Tooltip.Arrow className="fill-popover" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
