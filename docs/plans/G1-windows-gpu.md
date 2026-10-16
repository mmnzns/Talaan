# G1. Windows GPU (Vulkan) with automatic CPU fallback — task plan

> Branch `gpu-windows`, cut from `fix/1.0.3` (which has task 1, the x86-64-v3 CPU baseline). Research and design:
> the GPU research note of 2026-10-16 (Build Log). Nothing in this plan pushes or tags; CI is run by Miguel.

**Goal:** on a Windows PC with a discrete GPU, Whisper and the local summary AI run on it through Vulkan. On any
GPU problem the work is redone on the processor, and Talaan remembers it until the app or the graphics driver
changes. A PC without a GPU driver (Windows Sandbox, a VM) starts and works exactly as 1.0.x does.

**Architecture:**
- `talaan.exe` never links Vulkan. ggml registers its Vulkan backend even with the GPU off, and a missing
  `vulkan-1.dll` would stop the app at launch or crash it on first use. All GPU work runs in helper processes:
  - `talaan-llm-vulkan.exe`: the summary helper (`src-tauri/llm`) built with `--features vulkan`;
  - `talaan-stt-vulkan.exe`: a new Whisper helper (`src-tauri/stt`) built with `--features vulkan`.
  The CPU `talaan-llm.exe` and the in-process CPU Whisper stay as they are and are the fallback.
- Before starting a Vulkan helper the app checks that `vulkan-1.dll` loads from System32
  (`LoadLibraryExW(LOAD_LIBRARY_SEARCH_SYSTEM32)`), and starts it with `VK_LOADER_LAYERS_DISABLE=~implicit~`
  (overlay layers from Steam, Epic and others crash Vulkan apps).
- The helpers choose one device themselves: the discrete GPU with the most memory, never an integrated one, never
  split across devices (llama.cpp: `with_devices([i])`, split mode none, `fit_params` for partial offload; Whisper:
  `gpu_device(n)`). No discrete GPU means the helper reports `cpu` and the app uses the CPU path.
- Fallback: a GPU helper that fails to start, crashes, stalls, runs out of memory or returns an error has that
  summary (or speech chunk) redone on the CPU. When the CPU then succeeds, the failure is written to
  `<data>/gpu.json`, keyed by app version and graphics driver (display adapters' names and driver versions from the
  registry), and the GPU is skipped until either changes or the user clicks Try again in Check setup. A failure the
  CPU repeats is the input's fault, not the GPU's, and isn't remembered.
- `useGpu` (Settings › Speech models, default on) drives Whisper and the local summary AI. Check setup gets a
  "Graphics chip" row. The Mac is unchanged (Metal in-process); its row says "Apple GPU (Metal)".
- Build: `scripts/build-llm-helper.ps1` builds the GPU helpers when `VULKAN_SDK` is set (required with
  `TALAAN_GPU=1`), in their own target folder, with the same CPU baseline (`scripts/cpu-baseline.ps1`).
  `scripts/release-windows.ps1` bundles them through the release-only config's `bundle.resources`, checks their
  CMake caches (`GGML_VULKAN=ON`, `-O2`, x86-64-v3) and that `talaan.exe` doesn't import `vulkan-1.dll`. The NSIS
  hooks close the new helpers before install and uninstall.

**Tasks** (one commit each; all tests, `npx tsc --noEmit` and `npm run build` before each):

1. CPU baseline — done on `fix/1.0.3`.
2. Protocol: `Ready { backend, device, vram_mb }`, `gpu` on requests (default true), a `Devices` request, the
   Whisper helper's protocol, and the device policy (pure, unit-tested).
3. Summary helper GPU build: `vulkan` feature, device choice, `fit_params`, CPU when `gpu` is false.
4. Whisper helper `src-tauri/stt` and `RemoteWhisperEngine` (implements the transcriber's `Engine`).
5. App side: `gpu.rs` (System32 check, helper environment, `gpu.json`), summary fallback in `LocalAi`, speech
   fallback engine in the transcriber; fake `.cmd` helpers in tests.
6. Settings and Check setup: `gpu_status` / `gpu_retry` commands, the Graphics chip row, switch wording (en/fr/es).
7. Build scripts and installer: GPU helpers in `build-llm-helper.ps1`, release checks, NSIS hooks.
8. CI: Vulkan SDK 1.4.363.0 (pinned, cached), a `windows-gpu` job in `ci.yml` (manual) that uploads the bare
   helpers, and the Release workflow's test build with `TALAAN_GPU=1`.
9. Testing on Miguel's PC from the CI artifacts (docs/TESTING.md, "GPU on Windows").
10. Optional, last: Parakeet on DirectML, kept only if more than 2x faster with the same transcript.
