import type { ByokProvider } from "./settings";

/** Cloud models offered for "Your own key", the same set Parlato supports. The first of each list is what
 *  "Recommended" uses when the key can reach it (chosen for summaries: accurate over fastest). The Rust side
 *  keeps the same ids for request tuning (src-tauri/src/summarizer/engines.rs, checked by a test). */
export const BYOK_MODELS: Record<Exclude<ByokProvider, "compatible">, { id: string; name: string }[]> = {
  claude: [
    { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
    { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
    { id: "claude-fable-5-2", name: "Claude Fable 5.2" },
    { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
  ],
  openai: [
    { id: "gpt-6.2-sol", name: "GPT-6.2 Sol" },
    { id: "gpt-6.2-astra", name: "GPT-6.2 Astra" },
    { id: "gpt-6.1-sol", name: "GPT-6.1 Sol" },
    { id: "gpt-6.1-terra", name: "GPT-6.1 Terra" },
    { id: "gpt-6-luna", name: "GPT-6 Luna" },
  ],
  gemini: [
    { id: "gemini-4-hydrogen", name: "Gemini 4 Hydrogen" },
    { id: "gemini-4-neon", name: "Gemini 4 Neon" },
    { id: "gemini-4-argon", name: "Gemini 4 Argon" },
    { id: "gemini-4-carbon", name: "Gemini 4 Carbon" },
    { id: "gemini-3.8-flash", name: "Gemini 3.8 Flash" },
    { id: "gemini-3.5-flash-lite", name: "Gemini 3.5 Flash-Lite" },
  ],
  groq: [
    { id: "openai/gpt-oss-120b", name: "GPT-OSS 120B" },
    { id: "openai/gpt-oss-20b", name: "GPT-OSS 20B" },
    { id: "qwen/qwen3.8-27b", name: "Qwen 3.8 27B" },
  ],
  cerebras: [
    { id: "gpt-oss-120b", name: "GPT-OSS 120B" },
    { id: "qwen-3.8-27b", name: "Qwen 3.8 27B" },
  ],
  mistral: [
    { id: "mistral-large-latest", name: "Mistral Large" },
    { id: "mistral-small-latest", name: "Mistral Small" },
    { id: "ministral-14b-latest", name: "Ministral 14B" },
    { id: "ministral-8b-latest", name: "Ministral 8B" },
  ],
  openrouter: [
    { id: "anthropic/claude-sonnet-5.5", name: "Claude Sonnet 5.5" },
    { id: "anthropic/claude-opus-5.5", name: "Claude Opus 5.5" },
    { id: "anthropic/claude-haiku-5.5", name: "Claude Haiku 5.5" },
    { id: "anthropic/claude-fable-5.2", name: "Claude Fable 5.2" },
    { id: "openai/gpt-6.2-sol", name: "GPT-6.2 Sol" },
    { id: "openai/gpt-6-luna", name: "GPT-6 Luna" },
    { id: "openai/gpt-oss-120b", name: "GPT-OSS 120B" },
    { id: "google/gemini-4-hydrogen", name: "Gemini 4 Hydrogen" },
    { id: "google/gemini-4-neon", name: "Gemini 4 Neon" },
    { id: "google/gemini-3.8-flash", name: "Gemini 3.8 Flash" },
    { id: "deepseek/deepseek-v4.1-flash", name: "DeepSeek V4.1 Flash" },
    { id: "qwen/qwen3.8-flash", name: "Qwen 3.8 Flash" },
    { id: "mistralai/mistral-large-4-0", name: "Mistral Large 4" },
    { id: "x-ai/grok-4.7", name: "Grok 4.7" },
    { id: "meta-llama/llama-4-maverick", name: "Llama 4 Maverick" },
  ],
};

export function byokModels(provider: ByokProvider): { id: string; name: string }[] {
  return provider === "compatible" ? [] : BYOK_MODELS[provider];
}
