//! The speech model catalog and where each model's files live: Talaan's own copy, or a copy another app already
//! downloaded ("On this Mac", used read-only, never named in the UI).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Parakeet,
    Whisper,
    /// A GGUF model for the local summary AI (run by the `talaan-llm` helper).
    Llm,
    /// Speaker separation for the Them track.
    Diarization,
    /// Text embeddings for Ask's search (run by the `talaan-llm` helper).
    Embedding,
}

#[derive(Debug)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub maker: &'static str,
    pub note: &'static str,
    pub engine: EngineKind,
    /// Shown in Settings ("25 European languages").
    pub languages: &'static str,
    /// 1..3, for the dots in Settings.
    pub speed: u8,
    pub accuracy: u8,
    pub files: &'static [ModelFile],
    /// Folders, relative to another app's data folder, that may already hold these exact files.
    pub reuse_dirs: &'static [&'static str],
}

impl ModelInfo {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[cfg(test)]
const PARAKEET_V3: &str = "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/";
#[cfg(test)]
const WHISPER: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/";

pub static CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "parakeet-v3",
        name: "Parakeet",
        maker: "NVIDIA",
        note: "Fastest. Good for quick notes in 25 European languages.",
        engine: EngineKind::Parakeet,
        languages: "25 European languages",
        speed: 3,
        accuracy: 1,
        files: &[
            ModelFile { name: "encoder-model.int8.onnx", url: concat!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/", "encoder-model.int8.onnx"), size: 652_183_999, sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09" },
            ModelFile { name: "decoder_joint-model.int8.onnx", url: concat!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/", "decoder_joint-model.int8.onnx"), size: 18_202_004, sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70" },
            ModelFile { name: "nemo128.onnx", url: concat!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/", "nemo128.onnx"), size: 139_764, sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f" },
            ModelFile { name: "vocab.txt", url: concat!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/", "vocab.txt"), size: 93_939, sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d" },
            ModelFile { name: "config.json", url: concat!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/", "config.json"), size: 97, sha256: "666903c76b9798caf2c210afd4f6cd60b08a8dbf9800ec8d7a3bc0d2148ac466" },
        ],
        reuse_dirs: &["ParakeetModels/parakeet-tdt-0.6b-v3-int8"],
    },
    ModelInfo {
        id: "whisper-turbo",
        name: "Whisper Large Turbo",
        maker: "OpenAI",
        note: "Recommended. Most accurate, 99 languages including Filipino.",
        engine: EngineKind::Whisper,
        languages: "99 languages",
        speed: 2,
        accuracy: 3,
        files: &[ModelFile { name: "ggml-large-v3-turbo-q5_0.bin", url: concat!("https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/", "ggml-large-v3-turbo-q5_0.bin"), size: 574_041_195, sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2" }],
        reuse_dirs: &["Models"],
    },
    ModelInfo {
        id: "whisper-small",
        name: "Whisper Small",
        maker: "OpenAI",
        note: "Light and quick on older Macs. Lower accuracy.",
        engine: EngineKind::Whisper,
        languages: "99 languages",
        speed: 3,
        accuracy: 1,
        files: &[ModelFile { name: "ggml-small.bin", url: concat!("https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/", "ggml-small.bin"), size: 487_601_967, sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b" }],
        reuse_dirs: &["Models"],
    },
];

/// Silero voice activity detection, used with every model. Downloaded with the first model.
pub static VAD: ModelInfo = ModelInfo {
    id: "vad",
    name: "Silero VAD",
    maker: "Silero",
    note: "",
    engine: EngineKind::Whisper,
    languages: "",
    speed: 3,
    accuracy: 3,
    files: &[ModelFile { name: "ggml-silero-v6.2.0.bin", url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin", size: 885_098, sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987" }],
    reuse_dirs: &["Models"],
};

/// Whisper Turbo: on real calls it was more complete and accurate than Parakeet (which skipped stretches and
/// misheard more), covers Filipino, and at ~13x real time an hour-long call takes a few minutes.
/// Speaker separation (named speakers): NVIDIA Nemotron 3 Diarization, ONNX export for parakeet-rs.
pub static DIARIZATION: ModelInfo = ModelInfo {
    id: "diarization",
    name: "Speaker separation",
    maker: "NVIDIA",
    note: "Tells the other people on a call apart: Speaker 1, 2, 3. Up to 8 people.",
    engine: EngineKind::Diarization,
    languages: "Any language",
    speed: 3,
    accuracy: 2,
    files: &[ModelFile {
        name: "nemotron3_diar_v3.onnx",
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/4d2a8bc71f5c896ec40faa59732e6716295edaf2/nemotron-3-diarization/nemotron3_diar_v3.onnx",
        size: 400_506_656,
        sha256: "915e4fa23b0192ed9fadeb1cdd26847df986d50c92012d177be28d0343bbe03a",
    }],
    reuse_dirs: &[],
};

/// Ask's search by meaning: Qwen3-Embedding 0.6B, 100+ languages. Without it Ask searches by keywords.
pub static SEARCH: ModelInfo = ModelInfo {
    id: "search",
    name: "Search by meaning",
    maker: "Qwen",
    note: "Lets Ask find answers phrased differently from the question, in 100+ languages.",
    engine: EngineKind::Embedding,
    languages: "100+ languages",
    speed: 3,
    accuracy: 2,
    files: &[ModelFile {
        name: "Qwen3-Embedding-0.6B-Q8_0.gguf",
        url: "https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/370f27d7550e0def9b39c1f16d3fbaa13aa67728/Qwen3-Embedding-0.6B-Q8_0.gguf",
        size: 639_150_592,
        sha256: "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439",
    }],
    reuse_dirs: &[],
};

pub const DEFAULT_MODEL: &str = "whisper-turbo";

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    if id == VAD.id {
        return Some(&VAD);
    }
    if id == DIARIZATION.id {
        return Some(&DIARIZATION);
    }
    if id == SEARCH.id {
        return Some(&SEARCH);
    }
    CATALOG.iter().find(|m| m.id == id)
}

// Keep the URL prefixes referenced so a typo in one of the concat! literals above shows up in tests.
#[cfg(test)]
const PREFIXES: [&str; 2] = [PARAKEET_V3, WHISPER];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Missing,
    /// Talaan's own copy in `<data>/models/<id>/`.
    Own(PathBuf),
    /// Another app's copy, read in place.
    OnThisMac(PathBuf),
}

impl Location {
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Location::Own(p) | Location::OnThisMac(p) => Some(p),
            Location::Missing => None,
        }
    }
}

/// Where models are stored and which other apps' folders may already have them.
#[derive(Debug, Clone)]
pub struct ModelStore {
    pub root: PathBuf,
    pub reuse_roots: Vec<PathBuf>,
    pub reuse: bool,
}

impl ModelStore {
    pub fn new(data_dir: &Path, reuse: bool) -> Self {
        ModelStore { root: data_dir.join("models"), reuse_roots: default_reuse_roots(), reuse }
    }

    pub fn own_dir(&self, m: &ModelInfo) -> PathBuf {
        self.root.join(m.id)
    }

    pub fn locate(&self, m: &ModelInfo) -> Location {
        let own = self.own_dir(m);
        if complete(&own, m) {
            return Location::Own(own);
        }
        if self.reuse {
            for root in &self.reuse_roots {
                for rel in m.reuse_dirs {
                    let dir = root.join(rel);
                    if complete(&dir, m) {
                        return Location::OnThisMac(dir);
                    }
                }
            }
        }
        Location::Missing
    }
}

/// Every file present with its exact size. (Hashes are checked at download; re-hashing gigabytes on every
/// lookup would be too slow, and a size match on files this large is a strong signal.)
fn complete(dir: &Path, m: &ModelInfo) -> bool {
    m.files.iter().all(|f| std::fs::metadata(dir.join(f.name)).map(|md| md.is_file() && md.len() == f.size).unwrap_or(false))
}

/// Known data folders of other speech apps on this Mac. Confirmed layouts only.
fn default_reuse_roots() -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Some(d) = dirs::data_dir() {
        roots.push(d.join("com.craftconceptsdigital.parlato"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_model() -> &'static ModelInfo {
        find("whisper-small").unwrap()
    }

    fn sparse(path: &Path, size: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path).unwrap().set_len(size).unwrap();
    }

    #[test]
    fn catalog_is_consistent() {
        assert!(find(DEFAULT_MODEL).is_some());
        for m in CATALOG.iter().chain(std::iter::once(&VAD)) {
            assert!(!m.files.is_empty(), "{}", m.id);
            for f in m.files {
                assert_eq!(f.sha256.len(), 64, "{}", f.name);
                assert!(f.url.ends_with(f.name), "{}", f.url);
                assert!(f.url.starts_with("https://huggingface.co/"), "{}", f.url);
            }
        }
        for m in CATALOG.iter().filter(|m| m.engine == EngineKind::Parakeet) {
            assert!(m.files.iter().all(|f| f.url.starts_with(PREFIXES[0])));
        }
        for m in CATALOG.iter().filter(|m| m.engine == EngineKind::Whisper) {
            assert!(m.files.iter().all(|f| f.url.starts_with(PREFIXES[1])));
        }
    }

    #[test]
    fn own_copy_wins_and_partial_files_do_not_count() {
        let data = tempfile::tempdir().unwrap();
        let store = ModelStore { root: data.path().join("models"), reuse_roots: vec![], reuse: true };
        let m = fake_model();
        assert_eq!(store.locate(m), Location::Missing);
        sparse(&store.own_dir(m).join(m.files[0].name), m.files[0].size - 1);
        assert_eq!(store.locate(m), Location::Missing);
        sparse(&store.own_dir(m).join(m.files[0].name), m.files[0].size);
        assert_eq!(store.locate(m), Location::Own(store.own_dir(m)));
    }

    #[test]
    fn another_apps_copy_is_found_only_when_reuse_is_on() {
        let data = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let m = fake_model();
        sparse(&other.path().join("Models").join(m.files[0].name), m.files[0].size);
        let mut store = ModelStore { root: data.path().join("models"), reuse_roots: vec![other.path().to_path_buf()], reuse: true };
        assert_eq!(store.locate(m), Location::OnThisMac(other.path().join("Models")));
        store.reuse = false;
        assert_eq!(store.locate(m), Location::Missing);
    }
}
