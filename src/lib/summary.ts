import type { Summary } from "../data/types";

/** The user changed the summary or ticked an action item: writing it again would lose that. */
export function summaryHasChanges(summary: Summary | undefined): boolean {
  return Boolean(summary && (summary.edited || summary.actionItems.some((a) => a.done)));
}
