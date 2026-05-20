import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function shortId(s: string, n = 8): string {
  return s.length <= n ? s : `${s.slice(0, n)}…`;
}

export function relTime(value: string | number | null | undefined): string {
  if (value == null) return "—";
  // The REST surface returns timestamps in two shapes: ISO 8601
  // strings (legacy fields) and unix epoch seconds (canonical, see
  // `DOCS/openapi.yaml`). Accept both so a single helper covers
  // every column without callers having to format first.
  const t =
    typeof value === "number" ? value * 1000 : Date.parse(value);
  if (Number.isNaN(t)) return String(value);
  const sec = Math.round((Date.now() - t) / 1000);
  if (sec < 60) return `${sec}s ago`;
  if (sec < 3600) return `${Math.round(sec / 60)}m ago`;
  if (sec < 86400) return `${Math.round(sec / 3600)}h ago`;
  return `${Math.round(sec / 86400)}d ago`;
}
