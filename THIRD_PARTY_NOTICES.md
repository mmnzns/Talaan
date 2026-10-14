# Third-party notices

Talaan uses the following libraries, fonts and models. Each is used under its own license.

| Component | License | Source |
|---|---|---|
| Tauri | MIT / Apache-2.0 | https://tauri.app |
| React | MIT | https://react.dev |
| Vite | MIT | https://vite.dev |
| Tailwind CSS | MIT | https://tailwindcss.com |
| Bricolage Grotesque (font) | SIL OFL 1.1 | https://github.com/ateliertriay/bricolage |
| IBM Plex Mono (font) | SIL OFL 1.1 | https://github.com/IBM/plex |
| lucide-react (icons) | ISC | https://lucide.dev |
| clsx, tailwind-merge | MIT | https://github.com/lukeed/clsx, https://github.com/dcastil/tailwind-merge |
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

## Models

Models are downloaded on demand, not bundled.

| Model | License | Source |
|---|---|---|
| NVIDIA Parakeet TDT 0.6B v3 (ONNX export by istupakov) | CC-BY-4.0, © NVIDIA | https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3, https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx |
| OpenAI Whisper large-v3-turbo, small (ggml conversions) | MIT | https://github.com/openai/whisper, https://huggingface.co/ggerganov/whisper.cpp |
| Silero VAD (ggml conversion) | MIT | https://github.com/snakers4/silero-vad, https://huggingface.co/ggml-org/whisper-vad |

The local summary engine and other dependencies are added here as they are introduced.
