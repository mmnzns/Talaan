import { describe, expect, it } from "vitest";
import { messages } from "./index";

// Strings added with the shared engine (v0.2.2) once shipped English text in the French block and French text in the Spanish one.
// For these keys each language must differ from the other two.
const mustDiffer = [
  "settings.english_only",
  "settings.model_kept_one",
  "settings.model_kept_other",
  "settings.model_pending",
  "settings.setup_gpu_failed",
  "settings.setup_gpu_part_stt",
  "settings.setup_gpu_part_llm",
  "settings.local_option",
  "meeting.gpuFellBack",
];

describe("translated strings", () => {
  for (const key of mustDiffer) {
    it(`${key} is in three languages`, () => {
      const { en, fr, es } = { en: messages.en[key], fr: messages.fr[key], es: messages.es[key] };
      expect(en, key).toBeTruthy();
      expect(fr, key).not.toBe(en);
      expect(es, key).not.toBe(en);
      expect(es, key).not.toBe(fr);
    });
  }
});
