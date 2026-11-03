//! Talaan's speech helper (Whisper on Windows, processor or Vulkan build): the engine's helper with Talaan's prefix.
fn main() {
    voice_engine_stt::run_helper(voice_engine_protocol::HelperConfig { env_prefix: "TALAAN" });
}
