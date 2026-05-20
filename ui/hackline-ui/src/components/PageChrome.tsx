import * as React from "react";
import { cn } from "@/lib/utils";
import { HelpTip } from "@/components/HelpTip";

// Page chrome shared by every route. Centralises the title row so
// individual pages don't reinvent spacing. `help` renders a small
// `?` next to the title; the page owns the prose so it lives with
// the rest of the page.
export function PageHeader({
  title,
  description,
  actions,
  help,
}: {
  title: string;
  description?: string;
  actions?: React.ReactNode;
  help?: React.ReactNode;
}) {
  return (
    <div className="flex items-end justify-between border-b bg-background px-6 py-4">
      <div>
        <div className="flex items-center gap-2">
          <h1 className="text-base font-semibold leading-tight">{title}</h1>
          {help ? <HelpTip>{help}</HelpTip> : null}
        </div>
        {description ? (
          <p className="text-xs text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
    </div>
  );
}

export function PageBody({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex-1 overflow-auto p-6", className)}>{children}</div>
  );
}

export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed py-16 text-center">
      <div className="text-sm font-medium">{title}</div>
      {description ? (
        <div className="mt-1 max-w-md text-xs text-muted-foreground">
          {description}
        </div>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}

export function ErrorBox({ error }: { error: unknown }) {
  const msg = error instanceof Error ? error.message : String(error);
  return (
    <div className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {msg}
    </div>
  );
}
