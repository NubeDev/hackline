import { useEffect, useState } from "react";

// Minimal hash router. Hackline UI is a small set of pages and we
// don't want to pull react-router for it; the gateway also serves the
// bundle from a sub-path (`/admin`) so hash-based routes avoid the
// "every link must know the prefix" problem entirely.

export type Route =
  | { name: "devices" }
  | { name: "device"; id: number }
  | { name: "tunnels" }
  | { name: "cmd" }
  | { name: "events" }
  | { name: "audit" }
  | { name: "users" }
  | { name: "settings" };

export function parseRoute(hash: string): Route {
  const h = hash.replace(/^#\/?/, "");
  if (h === "" || h === "devices") return { name: "devices" };
  const m = h.match(/^devices\/(\d+)$/);
  if (m) return { name: "device", id: Number(m[1]) };
  if (h === "tunnels") return { name: "tunnels" };
  if (h === "cmd") return { name: "cmd" };
  if (h === "events") return { name: "events" };
  if (h === "audit") return { name: "audit" };
  if (h === "users") return { name: "users" };
  if (h === "settings") return { name: "settings" };
  return { name: "devices" };
}

export function routeHref(route: Route): string {
  switch (route.name) {
    case "devices":
      return "#/devices";
    case "device":
      return `#/devices/${route.id}`;
    case "tunnels":
      return "#/tunnels";
    case "cmd":
      return "#/cmd";
    case "events":
      return "#/events";
    case "audit":
      return "#/audit";
    case "users":
      return "#/users";
    case "settings":
      return "#/settings";
  }
}

export function navigate(route: Route): void {
  window.location.hash = routeHref(route);
}

export function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.hash));
  useEffect(() => {
    const onHash = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return route;
}
