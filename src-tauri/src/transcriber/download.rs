//! Model downloads: one file at a time into `<name>.part`, resumed with an HTTP Range request after an
//! interruption, checked against the catalog's size and sha256, then renamed into place.

use super::models::{ModelFile, ModelInfo};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
    /// No data for `IDLE` (a stalled connection). The part file is kept, so Retry resumes.
    #[error("the download of {0} stalled")]
    Stalled(String),
    /// The server answered with a web page instead of the model (a Wi-Fi sign-in page, a proxy).
    #[error("got a web page instead of {0}")]
    NotAModel(String),
}

impl DownloadError {
    /// The warning the app shows. `Display` stays English for logs.
    pub fn user_message(&self) -> String {
        use crate::i18n::{tr, tr_err, tr_with};
        match self {
            DownloadError::Cancelled => tr("error.cancelled"),
            DownloadError::Network(name, e) => tr_with("download.network", &[("name", name), ("error", e)]),
            DownloadError::Corrupt(name) => tr_with("download.corrupt", &[("name", name)]),
            DownloadError::Io(e) => tr_err("download.save_failed", e),
            DownloadError::Stalled(name) => tr_with("download.stalled", &[("name", name)]),
            DownloadError::NotAModel(name) => tr_with("download.not_a_model", &[("name", name)]),
        }
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::Io(e.to_string())
    }
}

/// A connection that delivers nothing for this long has stalled: the download stops with `Stalled` (Retry resumes).
pub const IDLE: Duration = Duration::from_secs(30);

/// Downloads every missing file of `model` into `dir`. `progress` gets 0..1 over the whole model.
pub fn download_model(model: &ModelInfo, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<(), DownloadError> {
    download_model_with(model, dir, cancel, IDLE, progress)
}

/// `download_model` with the stall timeout as a parameter (tests use a short one).
pub fn download_model_with(model: &ModelInfo, dir: &Path, cancel: &AtomicBool, idle: Duration, progress: &mut dyn FnMut(f32)) -> Result<(), DownloadError> {
    fs::create_dir_all(dir)?;
    let total = model.size().max(1) as f64;
    let mut done_before = 0u64;
    for file in model.files {
        let target = dir.join(file.name);
        let ready = fs::metadata(&target).map(|m| m.len() == file.size).unwrap_or(false);
        if !ready {
            let base = done_before;
            download_file(file, dir, cancel, idle, &mut |bytes| progress(((base + bytes) as f64 / total) as f32))?;
        }
        done_before += file.size;
        progress((done_before as f64 / total) as f32);
    }
    Ok(())
}

/// How much of `model` an interrupted download left in `dir` (whole files plus `.part` files), 0..1. None when
/// no `.part` file is there (nothing started, or the model is complete).
pub fn partial_fraction(model: &ModelInfo, dir: &Path) -> Option<f32> {
    let mut have = 0u64;
    let mut any_part = false;
    for file in model.files {
        if let Ok(m) = fs::metadata(dir.join(file.name)) {
            have += m.len().min(file.size);
        } else if let Ok(m) = fs::metadata(dir.join(format!("{}.part", file.name))) {
            any_part = true;
            have += m.len().min(file.size);
        }
    }
    (any_part && have > 0).then(|| (have as f64 / model.size().max(1) as f64) as f32)
}

/// The marker a Cancel leaves beside the part files. It survives a relaunch, so the app only resumes downloads
/// that a quit or a crash interrupted, never one the person stopped; the next Download clears it.
const CANCELLED: &str = ".cancelled";

pub fn mark_cancelled(dir: &Path) {
    if fs::create_dir_all(dir).is_ok() {
        let _ = fs::write(dir.join(CANCELLED), b"");
    }
}

pub fn clear_cancelled(dir: &Path) {
    let _ = fs::remove_file(dir.join(CANCELLED));
}

pub fn was_cancelled(dir: &Path) -> bool {
    dir.join(CANCELLED).exists()
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Talaan/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .expect("http client")
}

/// How often a waiting read looks at the cancel flag.
const POLL: Duration = Duration::from_millis(200);

/// A web page (a captive portal's sign-in page) is never a model file.
fn is_web_page(resp: &reqwest::Response) -> bool {
    resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.trim_start().to_ascii_lowercase().starts_with("text/html"))
}

fn download_file(file: &ModelFile, dir: &Path, cancel: &AtomicBool, idle: Duration, progress: &mut dyn FnMut(u64)) -> Result<(), DownloadError> {
    // A server that refuses to resume (416: the part file doesn't fit the file it has now) gets one fresh start.
    let first = fetch_file(file, dir, cancel, idle, progress);
    let result = match first {
        Err(Fetch::Restart) => {
            let _ = fs::remove_file(dir.join(format!("{}.part", file.name)));
            fetch_file(file, dir, cancel, idle, progress)
        }
        other => other,
    };
    match result {
        Ok(()) => Ok(()),
        Err(Fetch::Failed(e)) => Err(e),
        Err(Fetch::Restart) => Err(DownloadError::Network(file.name.into(), "HTTP 416".into())),
    }
}

/// Why `fetch_file` stopped: start over without the part file, or a real failure.
enum Fetch {
    Restart,
    Failed(DownloadError),
}

impl From<DownloadError> for Fetch {
    fn from(e: DownloadError) -> Self {
        Fetch::Failed(e)
    }
}

impl From<std::io::Error> for Fetch {
    fn from(e: std::io::Error) -> Self {
        Fetch::Failed(e.into())
    }
}

fn fetch_file(file: &ModelFile, dir: &Path, cancel: &AtomicBool, idle: Duration, progress: &mut dyn FnMut(u64)) -> Result<(), Fetch> {
    let part = dir.join(format!("{}.part", file.name));
    let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have > file.size {
        fs::remove_file(&part)?;
        have = 0;
    }

    // Hash what is already there so the final check covers the whole file. A large part file takes a while:
    // Cancel is looked at between chunks.
    let mut hasher = Sha256::new();
    if have > 0 {
        let mut f = File::open(&part)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(DownloadError::Cancelled.into());
            }
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }

    if have < file.size {
        let net = |e: String| DownloadError::Network(file.name.into(), e);
        // The network part runs on a small async runtime so a wait can be given up: at once on Cancel, and after
        // `idle` without data (a stalled connection would otherwise wait forever).
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| DownloadError::Io(e.to_string()))?;
        rt.block_on(async {
            let mut req = client().get(file.url);
            if have > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
            }
            let send = req.send();
            tokio::pin!(send);
            let mut waited = Duration::ZERO;
            let mut resp = loop {
                match tokio::time::timeout(POLL, &mut send).await {
                    Ok(r) => break r.map_err(|e| net(e.to_string()))?,
                    Err(_) if cancel.load(Ordering::Relaxed) => return Err(DownloadError::Cancelled.into()),
                    Err(_) => {
                        waited += POLL;
                        if waited >= idle {
                            return Err(DownloadError::Stalled(file.name.into()).into());
                        }
                    }
                }
            };
            let status = resp.status();
            // The status first: an error page (404, 5xx) is an HTTP error, not a Wi-Fi sign-in page.
            if have > 0 && status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                return Err(Fetch::Restart);
            }
            if !status.is_success() {
                return Err(net(format!("HTTP {status}")).into());
            }
            if is_web_page(&resp) {
                return Err(DownloadError::NotAModel(file.name.into()).into());
            }
            if have > 0 && status == reqwest::StatusCode::OK {
                // Server ignored the range: start over.
                have = 0;
                hasher = Sha256::new();
                File::create(&part)?;
            }
            let mut out = OpenOptions::new().create(true).append(true).open(&part)?;
            let mut since_report = 0usize;
            let mut quiet = Duration::ZERO;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    out.flush()?;
                    return Err(DownloadError::Cancelled.into());
                }
                let chunk = match tokio::time::timeout(POLL, resp.chunk()).await {
                    Ok(c) => c.map_err(|e| net(e.to_string()))?,
                    Err(_) => {
                        quiet += POLL;
                        if quiet >= idle {
                            out.flush()?;
                            return Err(DownloadError::Stalled(file.name.into()).into());
                        }
                        continue;
                    }
                };
                quiet = Duration::ZERO;
                let Some(chunk) = chunk else { break };
                out.write_all(&chunk)?;
                hasher.update(&chunk);
                have += chunk.len() as u64;
                since_report += chunk.len();
                if since_report >= 1 << 20 {
                    since_report = 0;
                    progress(have);
                }
            }
            out.flush()?;
            Ok(())
        })?;
    }

    let hash = format!("{:x}", hasher.finalize());
    if have != file.size || hash != file.sha256 {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::Corrupt(file.name.into()).into());
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

    /// Serves the first `send` bytes of `body`, then stops sending (the connection stays open). With `html`, it
    /// answers with a web page's content type, as a captive portal does.
    fn serve_stalling(body: Vec<u8>, send: usize, html: bool) -> String {
        struct Stall(Vec<u8>, usize);
        impl Read for Stall {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.1 >= self.0.len() {
                    thread::sleep(std::time::Duration::from_secs(20));
                    return Ok(0);
                }
                let n = buf.len().min(self.0.len() - self.1);
                buf[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
                self.1 += n;
                Ok(n)
            }
        }
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", server.server_addr().to_ip().unwrap());
        thread::spawn(move || {
            for req in server.incoming_requests() {
                let len = body.len();
                let first = body[..send].to_vec();
                let ct = if html { "text/html; charset=utf-8" } else { "application/octet-stream" };
                let header = tiny_http::Header::from_bytes("Content-Type", ct).unwrap();
                thread::spawn(move || {
                    let _ = req.respond(tiny_http::Response::new(200.into(), vec![header], Stall(first, 0), Some(len), None));
                });
            }
        });
        url
    }

    /// Answers a Range request with `range_code` and anything else with the whole body (200). With `html`, every
    /// answer is a web page with that code instead.
    fn serve_codes(body: Vec<u8>, range_code: u16, html: Option<u16>) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", server.server_addr().to_ip().unwrap());
        thread::spawn(move || {
            for req in server.incoming_requests() {
                if let Some(code) = html {
                    let h = tiny_http::Header::from_bytes("Content-Type", "text/html").unwrap();
                    let _ = req.respond(tiny_http::Response::from_string("<html>Not found</html>").with_status_code(code).with_header(h));
                    continue;
                }
                let ranged = req.headers().iter().any(|h| h.field.equiv("Range"));
                let r = if ranged { tiny_http::Response::from_data(vec![]).with_status_code(range_code) } else { tiny_http::Response::from_data(body.clone()) };
                let _ = req.respond(r);
            }
        });
        url
    }

    #[test]
    fn an_error_page_is_an_http_error_not_a_sign_in_page() {
        let body = body();
        let m = model_for(&serve_codes(body.clone(), 206, Some(404)), &body);
        let dir = tempfile::tempdir().unwrap();
        let r = download_model_with(m, dir.path(), &AtomicBool::new(false), Duration::from_secs(5), &mut |_| {});
        assert!(matches!(&r, Err(DownloadError::Network(_, e)) if e.contains("404")), "{r:?}");
    }

    #[test]
    fn a_refused_resume_starts_over() {
        let body = body();
        let m = model_for(&serve_codes(body.clone(), 416, None), &body);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin.part"), &body[..1_000_000]).unwrap();
        download_model_with(m, dir.path(), &AtomicBool::new(false), Duration::from_secs(5), &mut |_| {}).unwrap();
        assert_eq!(fs::read(dir.path().join("m.bin")).unwrap(), body);
    }

    #[test]
    fn cancel_is_seen_while_the_part_file_is_checked() {
        let body = body();
        let m = model_for("http://127.0.0.1:9/", &body);
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin.part"), &body[..2_500_000]).unwrap();
        let r = download_model_with(m, dir.path(), &AtomicBool::new(true), Duration::from_secs(5), &mut |_| {});
        assert_eq!(r, Err(DownloadError::Cancelled));
        assert!(dir.path().join("m.bin.part").exists());
    }

    #[test]
    fn a_stalled_connection_stops_with_an_error_and_keeps_the_part() {
        let body = body();
        let m = model_for(&serve_stalling(body.clone(), 1_500_000, false), &body);
        let dir = tempfile::tempdir().unwrap();
        let t = std::time::Instant::now();
        let r = download_model_with(m, dir.path(), &AtomicBool::new(false), Duration::from_secs(1), &mut |_| {});
        assert_eq!(r, Err(DownloadError::Stalled("m.bin".into())));
        assert!(t.elapsed() < Duration::from_secs(10));
        assert!(fs::metadata(dir.path().join("m.bin.part")).unwrap().len() > 0);
        assert!(partial_fraction(m, dir.path()).unwrap() > 0.0);
    }

    #[test]
    fn cancel_works_while_the_connection_is_stalled() {
        let body = body();
        let m = model_for(&serve_stalling(body.clone(), 100_000, false), &body);
        let dir = tempfile::tempdir().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(800));
            c.store(true, Ordering::Relaxed);
        });
        let t = std::time::Instant::now();
        let r = download_model_with(m, dir.path(), &cancel, Duration::from_secs(60), &mut |_| {});
        assert_eq!(r, Err(DownloadError::Cancelled));
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_web_page_is_not_saved_as_the_model() {
        let body = body();
        let m = model_for(&serve_stalling(body.clone(), body.len(), true), &body);
        let dir = tempfile::tempdir().unwrap();
        let r = download_model_with(m, dir.path(), &AtomicBool::new(false), Duration::from_secs(5), &mut |_| {});
        assert_eq!(r, Err(DownloadError::NotAModel("m.bin".into())));
        assert!(!dir.path().join("m.bin").exists());
    }

    #[test]
    fn a_cancel_is_remembered_until_the_next_download() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models/x");
        assert!(!was_cancelled(&models));
        mark_cancelled(&models);
        assert!(was_cancelled(&models));
        clear_cancelled(&models);
        assert!(!was_cancelled(&models));
    }

    #[test]
    fn partial_downloads_are_reported_and_finished_ones_are_not() {
        let body = body();
        let m = model_for("http://127.0.0.1:9/", &body);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(partial_fraction(m, dir.path()), None);
        fs::write(dir.path().join("m.bin.part"), &body[..1_500_000]).unwrap();
        assert!((partial_fraction(m, dir.path()).unwrap() - 0.5).abs() < 0.01);
        fs::remove_file(dir.path().join("m.bin.part")).unwrap();
        fs::write(dir.path().join("m.bin"), &body).unwrap();
        assert_eq!(partial_fraction(m, dir.path()), None);
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
