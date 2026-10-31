import { useEffect, useState } from "react";
import type { ExportScope } from "./share";

export type Route =
  | { name: "meetings" }
  | { name: "meeting"; id: string; t?: number; /** Open with the Export sheet showing (the menu bar's Export). */ export?: boolean }
  | { name: "ask"; meetingId?: string }
  | { name: "screens"; which?: string }
  | { name: "settings"; tab: string }
  | { name: "tray" }
  | { name: "prompt" }
  | { name: "print"; id: string; scope: ExportScope };

export function parseRoute(hash: string): Route {
  const [pathPart, queryPart = ""] = hash.replace(/^#/, "").split("?");
  const query = new URLSearchParams(queryPart);
  const seg = pathPart.split("/").filter(Boolean).map(decodeURIComponent);

  switch (seg[0]) {
    case "meeting":
      if (seg[1]) {
        const t = query.get("t");
        return { name: "meeting", id: seg[1], t: t ? Number(t) : undefined, ...(query.get("export") === "1" ? { export: true } : {}) };
      }
      return { name: "meetings" };
    case "ask":
      return { name: "ask", meetingId: query.get("m") ?? undefined };
    case "screens":
      return { name: "screens", which: seg[1] };
    case "settings":
      return { name: "settings", tab: seg[1] ?? "status" };
    case "tray":
      return { name: "tray" };
    case "print":
      return seg[1] ? { name: "print", id: seg[1], scope: query.get("scope") === "both" || query.get("scope") === "transcript" ? (query.get("scope") as ExportScope) : "summary" } : { name: "meetings" };
    case "prompt":
      return { name: "prompt" };
    default:
      return { name: "meetings" };
  }
}

export const routes = {
  meetings: () => "#/",
  meeting: (id: string, t?: number, exporting?: boolean) => {
    const query = [t !== undefined ? `t=${t}` : "", exporting ? "export=1" : ""].filter(Boolean).join("&");
    return `#/meeting/${encodeURIComponent(id)}${query ? `?${query}` : ""}`;
  },
  print: (id: string, scope: ExportScope) => `#/print/${encodeURIComponent(id)}${scope === "summary" ? "" : `?scope=${scope}`}`,
  ask: (meetingId?: string) => `#/ask${meetingId ? `?m=${encodeURIComponent(meetingId)}` : ""}`,
  screens: (which?: string) => `#/screens${which ? `/${which}` : ""}`,
  settings: (tab = "status") => `#/settings/${tab}`,
};

export function navigate(hash: string) {
  window.location.hash = hash;
}

export function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.hash));
  useEffect(() => {
    const on = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return route;
}
