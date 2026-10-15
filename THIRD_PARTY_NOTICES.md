# Third-party notices

Talaan uses the following libraries, fonts and models. Each is used under its own license.

| Component | License | Source |
|---|---|---|
| Tauri | MIT / Apache-2.0 | https://tauri.app |
| React | MIT | https://react.dev |
| Vite | MIT | https://vite.dev |
| Tailwind CSS | MIT | https://tailwindcss.com |
| Syne (font) | SIL OFL 1.1 | https://github.com/bonjour-monde/syne-typeface |
| Instrument Sans (font) | SIL OFL 1.1 | https://github.com/Instrument/instrument-sans |
| DM Mono (font) | SIL OFL 1.1 | https://github.com/googlefonts/dm-mono |
| Fontsource (font packaging) | MIT | https://fontsource.org |
| lucide-react (icons) | ISC | https://lucide.dev |
| clsx, tailwind-merge | MIT | https://github.com/lukeed/clsx, https://github.com/dcastil/tailwind-merge |
| tauri-plugin-single-instance | MIT / Apache-2.0 | https://github.com/tauri-apps/plugins-workspace |
| tauri-winrt-notification (Windows toasts) | MIT / Apache-2.0 | https://github.com/tauri-apps/winrt-notification |
| serde, serde_json | MIT / Apache-2.0 | https://serde.rs |
| chrono | MIT / Apache-2.0 | https://github.com/chronotope/chrono |
| dirs | MIT / Apache-2.0 | https://github.com/soc/dirs-rs |
| rand | MIT / Apache-2.0 | https://github.com/rust-random/rand |
| thiserror | MIT / Apache-2.0 | https://github.com/dtolnay/thiserror |
| tempfile (tests only) | MIT / Apache-2.0 | https://github.com/Stebalien/tempfile |
| cpal (audio input) | Apache-2.0 | https://github.com/RustAudio/cpal |
| hound (WAV files) | Apache-2.0 | https://github.com/ruuda/hound |
| rubato (resampling) | MIT / Apache-2.0 | https://github.com/HEnquist/rubato |
| rtrb (ring buffer) | MIT / Apache-2.0 | https://github.com/mgeier/rtrb |
| objc2, block2 (macOS bindings) | MIT | https://github.com/madsmtm/objc2 |
| objc2-foundation, objc2-core-foundation, objc2-core-audio, objc2-core-audio-types, objc2-av-foundation, objc2-app-kit | Zlib / Apache-2.0 / MIT | https://github.com/madsmtm/objc2 |
| mach2 | BSD-2-Clause / MIT / Apache-2.0 | https://github.com/JohnTitor/mach2 |
| whisper.cpp (via whisper-rs) | MIT | https://github.com/ggml-org/whisper.cpp |
| whisper-rs | Unlicense | https://github.com/tazz4843/whisper-rs |
| ONNX Runtime (via ort) | MIT | https://github.com/microsoft/onnxruntime |
| ort | MIT / Apache-2.0 | https://github.com/pykeio/ort |
| parakeet-rs | MIT / Apache-2.0 | https://github.com/altunenes/parakeet-rs |
| reqwest (model downloads) | MIT / Apache-2.0 | https://github.com/seanmonstar/reqwest |
| sha2 | MIT / Apache-2.0 | https://github.com/RustCrypto/hashes |
| strsim | MIT | https://github.com/rapidfuzz/strsim-rs |
| whatlang (language detection) | MIT | https://github.com/greyblake/whatlang-rs |
| tiny_http (tests only) | MIT / Apache-2.0 | https://github.com/tiny-http/tiny-http |
| llama.cpp (via llama-cpp-2, in the talaan-llm helper) | MIT | https://github.com/ggml-org/llama.cpp |
| llama-cpp-2 | MIT / Apache-2.0 | https://github.com/utilityai/llama-cpp-rs |
| keyring (Keychain) | MIT / Apache-2.0 | https://github.com/hwchen/keyring-rs |
| objc2-user-notifications | Zlib / Apache-2.0 / MIT | https://github.com/madsmtm/objc2 |
| Symphonia (audio import) | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| tauri-plugin-dialog, -opener, -updater, -autostart | MIT / Apache-2.0 | https://github.com/tauri-apps/plugins-workspace |
| objc2-event-kit (macOS calendar) | Zlib / Apache-2.0 / MIT | https://github.com/madsmtm/objc2 |
| wasapi (Windows system audio) | MIT | https://github.com/HEnquist/wasapi-rs |
| winreg (Windows registry) | MIT | https://github.com/gentoo90/winreg-rs |
| windows (Windows bindings) | MIT / Apache-2.0 | https://github.com/microsoft/windows-rs |
| rrule (recurring events) | MIT / Apache-2.0 | https://github.com/fmeringdal/rust-rrule |
| chrono-tz (time zones; includes the IANA tz database, public domain) | MIT / Apache-2.0 | https://github.com/chronotope/chrono-tz |

## Models

Models are downloaded on demand from their sources, not bundled or redistributed by Talaan. Their license terms travel with them on the source pages listed.

| Model | License | Source |
|---|---|---|
| NVIDIA Parakeet TDT 0.6B v3 (ONNX export by istupakov) | CC-BY-4.0, © NVIDIA | https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3, https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx |
| OpenAI Whisper large-v3-turbo, small (ggml conversions) | MIT | https://github.com/openai/whisper, https://huggingface.co/ggerganov/whisper.cpp |
| Silero VAD (ggml conversion) | MIT | https://github.com/snakers4/silero-vad, https://huggingface.co/ggml-org/whisper-vad |
| NVIDIA Nemotron 3 Diarization (streaming Sortformer; ONNX export by altunenes for parakeet-rs) | OpenMDW-1.1, © NVIDIA | https://huggingface.co/nvidia, https://huggingface.co/altunenes/parakeet-rs (`nemotron-3-diarization/`), license text: https://openmdw.ai/license |
| Qwen3-Embedding 0.6B (GGUF, Q8_0) | Apache-2.0 | https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF |
| Qwen3.5 4B and 9B (GGUF quantizations by Unsloth) | Apache-2.0 | https://huggingface.co/Qwen/Qwen3.5-4B, https://huggingface.co/Qwen/Qwen3.5-9B, https://huggingface.co/unsloth |

Other dependencies are added here as they are introduced.
