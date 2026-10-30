//! Talaan's local AI helper: the engine's llama.cpp helper, with Talaan's environment prefix.
fn main() {
    voice_engine_llm::run(voice_engine_protocol::HelperConfig { env_prefix: "TALAAN" });
}
