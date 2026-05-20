import { routeHref, useRoute, type Route } from "@/lib/route";
import { cn } from "@/lib/utils";

const items: { route: Route; label: string }[] = [
  { route: { name: "devices" }, label: "Devices" },
  { route: { name: "tunnels" }, label: "Tunnels" },
  { route: { name: "cmd" }, label: "Cmd outbox" },
  { route: { name: "events" }, label: "Live events" },
  { route: { name: "audit" }, label: "Audit" },
  { route: { name: "users" }, label: "Users" },
  { route: { name: "settings" }, label: "Settings" },
];

function isActive(active: Route, item: Route): boolean {
  if (active.name === item.name) return true;
  // Device detail counts as the Devices section being active.
  if (active.name === "device" && item.name === "devices") return true;
  return false;
}

export function Sidebar() {
  const active = useRoute();
  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r bg-card">
      <div className="px-4 py-4">
        <div className="text-sm font-semibold tracking-tight">hackline</div>
        <div className="text-[11px] text-muted-foreground">fleet · gateway admin</div>
      </div>
      <nav className="flex flex-col gap-0.5 px-2 pb-4">
        {items.map((it) => (
          <a
            key={it.route.name}
            href={routeHref(it.route)}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm transition-colors",
              isActive(active, it.route)
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
            )}
          >
            {it.label}
          </a>
        ))}
      </nav>
    </aside>
  );
}
