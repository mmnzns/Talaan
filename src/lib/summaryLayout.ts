import type { ActionItem, Meeting, Summary, SummaryTakeaway } from "../data/types";
import { t } from "./i18n";

/** Whether a summary uses the detailed layout (purpose, key takeaways, topics). Older summaries and the other meeting types don't. */
export function isDetailed(s: Pick<Summary, "purpose" | "takeaways" | "topics">): boolean {
  return !!s.purpose?.trim() || (s.takeaways?.length ?? 0) > 0 || (s.topics?.length ?? 0) > 0;
}

export interface NextStepGroup {
  /** The person as shown (Your name for the user), or undefined for the items nobody owns. */
  owner?: string;
  items: ActionItem[];
}

/** The name the user gave themselves (Your name), or undefined while it is empty or still the stored default. */
function yourName(meeting: Pick<Meeting, "speakers">): string | undefined {
  const named = meeting.speakers.me?.trim();
  return named && named !== "Me" ? named : undefined;
}

/**
 * A task's text for putting a due date or a time after it: without the trailing period or space the summary AI often
 * ends it with ("Confirm the estimate with engineering." would read "engineering., due Thursday"). Untouched when
 * nothing follows it.
 */
export function taskText(a: Pick<ActionItem, "text" | "due" | "at">): string {
  const text = a.text.trimEnd();
  return a.due?.trim() || a.at !== undefined ? text.replace(/[.\s]+$/, "") || text : text;
}

/**
 * The action items grouped by person: the user's own group first (Your name, or "You" while it is empty; the same
 * person written as Me and then Your name counts once), then the others in the order they first appear (the same person
 * in another case counts once), items nobody owns last. Items keep their order inside a group.
 */
export function nextStepsByOwner(meeting: Pick<Meeting, "speakers">, items: ActionItem[]): NextStepGroup[] {
  const you = yourName(meeting);
  const youLabel = you ?? t("meeting.youGroup");
  const groups: NextStepGroup[] = [];
  const isYou = (g: NextStepGroup) => g.owner === youLabel;
  for (const a of items) {
    const raw = a.owner?.trim();
    const lower = raw?.toLowerCase();
    const owner = !raw ? undefined : lower === "me" || lower === you?.toLowerCase() ? youLabel : raw;
    const group = groups.find((g) => g.owner?.toLowerCase() === owner?.toLowerCase());
    if (group) group.items.push(a);
    else groups.push({ owner, items: [a] });
  }
  return [...groups.filter(isYou), ...groups.filter((g) => g.owner !== undefined && !isYou(g)), ...groups.filter((g) => g.owner === undefined)];
}

/** The purpose, then each takeaway as "Label: text", as plain sentences (what the stored recap holds; mirrors the Rust side). */
export function recapFrom(purpose: string | undefined, takeaways: SummaryTakeaway[] | undefined): string {
  const parts = [purpose?.trim() ?? "", ...(takeaways ?? []).map((t) => (t.label.trim() ? `${t.label.trim().replace(/:$/, "")}: ${t.text}` : t.text))].filter(Boolean);
  return parts.map((p) => (/[.!?。]$/.test(p) ? p : `${p}.`)).join(" ");
}
