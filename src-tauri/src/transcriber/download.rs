//! Model downloads: one file at a time into `<name>.part`, resumed with an HTTP Range request after an
//! interruption, checked against the catalog's size and sha256, then renamed into place.

use super::models::{ModelFile, ModelInfo};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DownloadError {
    #[error("download cancelled")]
    Cancelled,
    #[error("couldn't download {0}: {1}")]
    Network(String, String),
    #[error("{0} was damaged in transit, try again")]
    Corrupt(String),
    #[error("couldn't save the model: {0}")]
    Io(String),
}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::Io(e.to_string())
    }
}

/// Downloads every missing file of `model` into `dir`. `progress` gets 0..1 over the whole model.
pub fn download_model(model: &ModelInfo, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<(), DownloadError> {
    fs::create_dir_all(dir)?;
    let total = model.size().max(1) as f64;
    let mut done_before = 0u64;
    for file in model.files {
        let target = dir.join(file.name);
        let ready = fs::metadata(&target).map(|m| m.len() == file.size).unwrap_or(false);
        if !ready {
            let base = done_before;
            download_file(file, dir, cancel, &mut |bytes| progress(((base + bytes) as f64 / total) as f32))?;
        }
        done_before += file.size;
        progress((done_before as f64 / total) as f32);
    }
    Ok(())
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("Talaan/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(None)
        .build()
        .expect("http client")
}

fn download_file(file: &ModelFile, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64)) -> Result<(), DownloadError> {
    let part = dir.join(format!("{}.part", file.name));
    let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have > file.size {
        fs::remove_file(&part)?;
        have = 0;
    }

    // Hash what is already there so the final check covers the whole file.
    let mut hasher = Sha256::new();
    if have > 0 {
        let mut f = File::open(&part)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }

    if have < file.size {
        let mut req = client().get(file.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().map_err(|e| DownloadError::Network(file.name.into(), e.to_string()))?;
        let status = resp.status();
        if have > 0 && status == reqwest::StatusCode::OK {
            // Server ignored the range: start over.
            have = 0;
            hasher = Sha256::new();
            File::create(&part)?;
        } else if !status.is_success() {
            return Err(DownloadError::Network(file.name.into(), format!("HTTP {status}")));
        }
        let mut out = OpenOptions::new().create(true).append(true).open(&part)?;
        let mut buf = vec![0u8; 1 << 16];
        let mut since_report = 0usize;
        loop {
            if cancel.load(Ordering::Relaxed) {
                out.flush()?;
                return Err(DownloadError::Cancelled);
            }
            let n = resp.read(&mut buf).map_err(|e| DownloadError::Network(file.name.into(), e.to_string()))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            have += n as u64;
            since_report += n;
            if since_report >= 1 << 20 {
                since_report = 0;
                progress(have);
            }
        }
        out.flush()?;
    }

    let hash = format!("{:x}", hasher.finalize());
    if have != file.size || hash != file.sha256 {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::Corrupt(file.name.into()));
    }
    fs::rename(&part, dir.join(file.name))?;
    progress(have);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::models::EngineKind;
    use std::sync::Arc;
    use std::thread;

    /// A tiny HTTP server serving `body` with Range support. Returns its base URL.
    fn serve(body: Vec<u8>, honour_range: bool) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", server.server_addr().to_ip().unwrap());
        let body = Arc::new(body);
        thread::spawn(move || {
            for req in server.incoming_requests() {
                let start = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Range"))
                    .and_then(|h| h.value.as_str().strip_prefix("bytes=")?.trim_end_matches('-').parse::<usize>().ok())
                    .filter(|_| honour_range);
                let (code, data) = match start {
                    Some(s) => (206, body[s..].to_vec()),
                    None => (200, body.to_vec()),
                };
                let _ = req.respond(tiny_http::Response::from_data(data).with_status_code(code));
            }
        });
        url
    }

    fn model_for(url: &str, body: &[u8]) -> &'static ModelInfo {
        let sha = format!("{:x}", Sha256::digest(body));
        let file = Box::leak(Box::new(ModelFile { name: "m.bin", url: Box::leak(format!("{url}m.bin").into_boxed_str()), size: body.len() as u64, sha256: Box::leak(sha.into_boxed_str()) }));
        Box::leak(Box::new(ModelInfo {
            id: "test",
            name: "Test",
            maker: "",
            note: "",
            engine: EngineKind::Whisper,
            languages: "",
            language_codes: &[],
            speed: 1,
            accuracy: 1,
            files: std::slice::from_ref(file),
            reuse_dirs: &[],
        }))
    }

    fn body() -> Vec<u8> {
        (0..3_000_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn downloads_checks_and_reports_progress() {
        let body = body();
        let m = model_for(&serve(body.clone(), true), &body);
        let dir = tempfile::tempdir().unwrap();
        let mut last = 0.0;
        download_model(m, dir.path(), &AtomicBool::new(false), &mut |p| last = p).unwrap();
        assert_eq!(fs::read(dir.path().join("m.bin")).unwrap(), body);
        assert!(!dir.path().join("m.bin.part").exists());
        assert_eq!(last, 1.0);
    }

    #[test]
    fn resumes_a_partial_download() {
        let body = body();
        let m = model_for(&serve(body.clone(), true), &body);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin.part"), &body[..1_000_000]).unwrap();
        download_model(m, dir.path(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(fs::read(dir.path().join("m.bin")).unwrap(), body);
    }

    #[test]
    fn starts_over_when_the_server_ignores_the_range() {
        let body = body();
        let m = model_for(&serve(body.clone(), false), &body);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin.part"), &body[..1_000_000]).unwrap();
        download_model(m, dir.path(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(fs::read(dir.path().join("m.bin")).unwrap(), body);
    }

    #[test]
    fn a_damaged_file_is_rejected_and_removed() {
        let body = body();
        let m = model_for(&serve(body.clone(), true), &body);
        let dir = tempfile::tempdir().unwrap();
        let mut bad = body[..1_000_000].to_vec();
        bad[10] ^= 0xff;
        fs::write(dir.path().join("m.bin.part"), &bad).unwrap();
        assert_eq!(download_model(m, dir.path(), &AtomicBool::new(false), &mut |_| {}), Err(DownloadError::Corrupt("m.bin".into())));
        assert!(!dir.path().join("m.bin").exists());
        assert!(!dir.path().join("m.bin.part").exists());
    }

    #[test]
    fn cancel_keeps_the_partial_file_for_later() {
        let body = body();
        let m = model_for(&serve(body.clone(), true), &body);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(download_model(m, dir.path(), &AtomicBool::new(true), &mut |_| {}), Err(DownloadError::Cancelled));
        assert!(!dir.path().join("m.bin").exists());
    }

    /// Downloads real models into the app's data folder. `TALAAN_MODELS="parakeet-v3 vad" cargo test real_download -- --ignored`
    #[test]
    #[ignore]
    fn real_download() {
        use crate::transcriber::models::{find, ModelStore};
        let data = std::env::var("TALAAN_DATA_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| dirs::data_dir().unwrap().join("Talaan"));
        let store = ModelStore::new(&data, false);
        for id in std::env::var("TALAAN_MODELS").unwrap_or("vad parakeet-v3".into()).split_whitespace() {
            let m = find(id).unwrap();
            let t = std::time::Instant::now();
            let mut last = -1i32;
            download_model(m, &store.own_dir(m), &AtomicBool::new(false), &mut |p| {
                let pct = (p * 100.0) as i32;
                if pct / 10 != last / 10 {
                    last = pct;
                    eprintln!("{id}: {pct}%");
                }
            })
            .unwrap();
            eprintln!("{id}: done in {:.0}s", t.elapsed().as_secs_f32());
        }
    }
}
