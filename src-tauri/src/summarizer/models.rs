//! Local summary AI models (GGUF, run by the `talaan-llm` helper). Picked by a bake-off on this Mac
//! (docs/plans/M5-summarizer.md): Qwen3.5 4B got every decision and owner right with deadlines as spoken;
//! 9B is a little better on action items for twice the size and time.

use crate::transcriber::models::{EngineKind, ModelFile, ModelInfo};

pub static LLM_CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "qwen3.5-4b",
        name: "Qwen3.5 4B",
        maker: "Qwen",
        note: "Recommended. Accurate decisions and action items, about 30 s for a short call.",
        engine: EngineKind::Llm,
        languages: "Many languages",
        speed: 3,
        accuracy: 2,
        files: &[ModelFile {
            name: "Qwen3.5-4B-Q4_K_M.gguf",
            url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a09523/Qwen3.5-4B-Q4_K_M.gguf",
            size: 2_740_937_888,
            sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        }],
        reuse_dirs: &[],
    },
    ModelInfo {
        id: "qwen3.5-9b",
        name: "Qwen3.5 9B",
        maker: "Qwen",
        note: "Larger. The most complete action items; needs 16 GB of memory and takes longer.",
        engine: EngineKind::Llm,
        languages: "Many languages",
        speed: 2,
        accuracy: 3,
        files: &[ModelFile {
            name: "Qwen3.5-9B-Q4_K_M.gguf",
            url: "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/3885219b6810b007914f3a7950a8d1b469d598a5/Qwen3.5-9B-Q4_K_M.gguf",
            size: 5_680_522_464,
            sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
        }],
        reuse_dirs: &[],
    },
];

/// Settings value ("recommended" | "larger") to a model.
pub fn local_model(setting: &str) -> &'static ModelInfo {
    match setting {
        "larger" => &LLM_CATALOG[1],
        _ => &LLM_CATALOG[0],
    }
}

pub fn find_llm(id: &str) -> Option<&'static ModelInfo> {
    LLM_CATALOG.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        for m in LLM_CATALOG {
            assert_eq!(m.engine, EngineKind::Llm);
            for f in m.files {
                assert_eq!(f.sha256.len(), 64);
                assert!(f.url.ends_with(f.name));
            }
        }
        assert_eq!(local_model("recommended").id, "qwen3.5-4b");
        assert_eq!(local_model("larger").id, "qwen3.5-9b");
        assert_eq!(local_model("anything").id, "qwen3.5-4b");
    }
}
