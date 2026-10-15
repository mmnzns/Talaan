import type { ByokProvider } from "./settings";

/** Cloud models offered for "Your own key", the same set Parlato supports. The first of each list is what
 *  "Recommended" uses when the key can reach it (chosen for summaries: accurate over fastest). The Rust side
 *  keeps the same ids for request tuning (src-tauri/src/summarizer/engines.rs, checked by a test). */
export const BYOK_MODELS: Record<Exclude<ByokProvider, "compatible">, { id: string; name: string }[]> = {
  claude: [
    { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
    { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
    { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
    { id: "claude-fable-5-1", name: "Claude Fable 5.1" },
  ],
  openai: [
    { id: "gpt-6-sol", name: "GPT-6 Sol" },
    { id: "gpt-6.1-sol", name: "GPT-6.1 Sol" },
    { id: "gpt-6-luna", name: "GPT-6 Luna" },
    { id: "gpt-6-astra", name: "GPT-6 Astra" },
    { id: "gpt-5.6-sol", name: "GPT-5.6 Sol" },
    { id: "gpt-5.6-terra", name: "GPT-5.6 Terra" },
    { id: "gpt-5.6-luna", name: "GPT-5.6 Luna" },
    { id: "gpt-5.5", name: "GPT-5.5" },
    { id: "gpt-5.4", name: "GPT-5.4" },
    { id: "gpt-5.4-mini", name: "GPT-5.4 mini" },
  ],
  gemini: [
    { id: "gemini-3.8-flash", name: "Gemini 3.8 Flash" },
    { id: "gemini-3.1-pro-preview", name: "Gemini 3.1 Pro (preview)" },
    { id: "gemini-3.6-flash", name: "Gemini 3.6 Flash" },
    { id: "gemini-3.5-flash-lite", name: "Gemini 3.5 Flash-Lite" },
    { id: "gemini-3.1-flash-lite", name: "Gemini 3.1 Flash-Lite" },
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
    { id: "mistral-medium-latest", name: "Mistral Medium" },
    { id: "mistral-large-latest", name: "Mistral Large" },
    { id: "mistral-small-latest", name: "Mistral Small" },
    { id: "ministral-14b-latest", name: "Ministral 14B" },
    { id: "ministral-8b-latest", name: "Ministral 8B" },
  ],
  openrouter: [
    { id: "anthropic/claude-sonnet-5.5", name: "Claude Sonnet 5.5" },
    { id: "anthropic/claude-opus-5.5", name: "Claude Opus 5.5" },
    { id: "anthropic/claude-haiku-5.5", name: "Claude Haiku 5.5" },
    { id: "openai/gpt-6-sol", name: "GPT-6 Sol" },
    { id: "openai/gpt-6-luna", name: "GPT-6 Luna" },
    { id: "openai/gpt-oss-120b", name: "GPT-OSS 120B" },
    { id: "google/gemini-3.8-flash", name: "Gemini 3.8 Flash" },
    { id: "google/gemini-3.1-pro-preview", name: "Gemini 3.1 Pro (preview)" },
    { id: "deepseek/deepseek-v4.1-flash", name: "DeepSeek V4.1 Flash" },
    { id: "qwen/qwen3.8-flash", name: "Qwen 3.8 Flash" },
    { id: "mistralai/mistral-medium-3-5", name: "Mistral Medium 3.5" },
    { id: "x-ai/grok-4.7", name: "Grok 4.7" },
    { id: "meta-llama/llama-4-maverick", name: "Llama 4 Maverick" },
  ],
};

export function byokModels(provider: ByokProvider): { id: string; name: string }[] {
  return provider === "compatible" ? [] : BYOK_MODELS[provider];
}
