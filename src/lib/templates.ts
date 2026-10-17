import type { MessageKey } from "./i18n";

/** Meeting types, the same list as src-tauri/src/summarizer/templates.rs. General is today's fixed format. */
export const TEMPLATES = ["general", "one_on_one", "standup", "interview", "sales", "lecture"] as const;
export type TemplateId = (typeof TEMPLATES)[number];

export const isTemplate = (id: string | undefined): id is TemplateId => TEMPLATES.includes(id as TemplateId);

/** The name shown for a meeting type; unknown ids (from a newer build) read as General. */
export const templateName = (id: string | undefined): MessageKey => `common.template_${isTemplate(id) ? id : "general"}` as MessageKey;
