import type { ActionItem, Meeting, Summary, SummaryTakeaway } from "../data/types";
import { ownerName } from "./transcript";

/** Whether a summary uses the detailed layout (purpose, key takeaways, topics). Older summaries and the other meeting types don't. */
export function isDetailed(s: Pick<Summary, "purpose" | "takeaways" | "topics">): boolean {
  return !!s.purpose?.trim() || (s.takeaways?.length ?? 0) > 0 || (s.topics?.length ?? 0) > 0;
}

export interface NextStepGroup {
  /** The person as shown (Your name for the user), or undefined for the items nobody owns. */
  owner?: string;
  items: ActionItem[];
}

/**
 * The action items grouped by person: owners in the order they first appear (the same person written in another case,
 * or as Me and then Your name, counts once), items nobody owns last. Items keep their order inside a group.
 */
export function nextStepsByOwner(meeting: Pick<Meeting, "speakers">, items: ActionItem[]): NextStepGroup[] {
  const groups: NextStepGroup[] = [];
  for (const a of items) {
    const owner = a.owner?.trim() ? ownerName(meeting, a.owner).trim() : undefined;
    const group = groups.find((g) => g.owner?.toLowerCase() === owner?.toLowerCase());
    if (group) group.items.push(a);
    else groups.push({ owner, items: [a] });
  }
  return [...groups.filter((g) => g.owner !== undefined), ...groups.filter((g) => g.owner === undefined)];
}

/** The purpose, then each takeaway as "Label: text", as plain sentences (what the stored recap holds; mirrors the Rust side). */
export function recapFrom(purpose: string | undefined, takeaways: SummaryTakeaway[] | undefined): string {
  const parts = [purpose?.trim() ?? "", ...(takeaways ?? []).map((t) => (t.label.trim() ? `${t.label.trim().replace(/:$/, "")}: ${t.text}` : t.text))].filter(Boolean);
  return parts.map((p) => (/[.!?。]$/.test(p) ? p : `${p}.`)).join(" ");
}
