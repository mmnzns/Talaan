//! Each meeting's passage vectors, saved beside it (`search.bin`) so a question only embeds itself.
//! The file carries a fingerprint of the model and the passages' text: an edit, a new summary or another
//! search model makes it stale, and it is rebuilt on the next question (or by the background indexer).

use super::passages::Passage;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const FILE: &str = "search.bin";
const MAGIC: &[u8; 4] = b"TLSI";
const VERSION: u32 = 1;
/// Texts per helper request: small enough to cancel promptly.
const BATCH: usize = 32;

pub type Fingerprint = [u8; 32];

pub fn fingerprint(model_id: &str, passages: &[Passage]) -> Fingerprint {
    let mut h = Sha256::new();
    h.update(model_id.as_bytes());
    for p in passages {
        h.update([0u8]);
        h.update(p.text.as_bytes());
    }
    h.finalize().into()
}

/// The saved vectors, when they match `fp`.
pub fn load(dir: &Path, fp: &Fingerprint) -> Option<Vec<Vec<f32>>> {
    let bytes = std::fs::read(dir.join(FILE)).ok()?;
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    if bytes.get(..4)? != MAGIC || u32_at(4)? != VERSION || bytes.get(8..40)? != fp {
        return None;
    }
    let (dims, count) = (u32_at(40)? as usize, u32_at(44)? as usize);
    let body = bytes.get(48..)?;
    if body.len() != dims * count * 4 {
        return None;
    }
    let floats: Vec<f32> = body.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
    Some(floats.chunks(dims.max(1)).map(<[f32]>::to_vec).take(count).collect())
}

pub fn save(dir: &Path, fp: &Fingerprint, vectors: &[Vec<f32>]) -> std::io::Result<()> {
    let dims = vectors.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(48 + vectors.len() * dims * 4);
    out.extend(MAGIC);
    out.extend(VERSION.to_le_bytes());
    out.extend(fp);
    out.extend((dims as u32).to_le_bytes());
    out.extend((vectors.len() as u32).to_le_bytes());
    for v in vectors {
        for x in v {
            out.extend(x.to_le_bytes());
        }
    }
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, out)?;
    std::fs::rename(tmp, dir.join(FILE))
}

/// The meeting's vectors: from disk when current, else embedded now and saved.
pub fn ensure(dir: &Path, model_id: &str, passages: &[Passage], embed: &mut dyn FnMut(Vec<String>) -> Result<Vec<Vec<f32>>, String>) -> Result<Vec<Vec<f32>>, String> {
    let fp = fingerprint(model_id, passages);
    if let Some(v) = load(dir, &fp).filter(|v| v.len() == passages.len()) {
        return Ok(v);
    }
    let mut vectors = Vec::with_capacity(passages.len());
    for chunk in passages.chunks(BATCH) {
        let got = embed(chunk.iter().map(|p| p.text.clone()).collect())?;
        if got.len() != chunk.len() {
            return Err("the search model returned the wrong number of vectors".into());
        }
        vectors.extend(got);
    }
    // A failed save only costs re-embedding next time.
    let _ = save(dir, &fp, &vectors);
    Ok(vectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Passage {
        Passage { meeting_id: "m".into(), at: Some(0), text: text.into(), lines: vec![] }
    }

    #[test]
    fn vectors_are_embedded_once_then_read_back() {
        let d = tempfile::tempdir().unwrap();
        let ps = vec![p("one"), p("two")];
        let mut calls = 0;
        let mut embed = |texts: Vec<String>| {
            calls += 1;
            Ok(texts.iter().map(|t| vec![t.len() as f32, 0.5]).collect())
        };
        let a = ensure(d.path(), "m1", &ps, &mut embed).unwrap();
        let b = ensure(d.path(), "m1", &ps, &mut embed).unwrap();
        assert_eq!(a, vec![vec![3.0, 0.5], vec![3.0, 0.5]]);
        assert_eq!(a, b);
        assert_eq!(calls, 1);
    }

    #[test]
    fn an_edit_or_another_model_makes_the_index_stale() {
        let d = tempfile::tempdir().unwrap();
        let mut calls = 0;
        let mut embed = |texts: Vec<String>| {
            calls += 1;
            Ok(texts.iter().map(|_| vec![1.0]).collect())
        };
        ensure(d.path(), "m1", &[p("one")], &mut embed).unwrap();
        ensure(d.path(), "m1", &[p("one, edited")], &mut embed).unwrap();
        ensure(d.path(), "m2", &[p("one, edited")], &mut embed).unwrap();
        assert_eq!(calls, 3);
    }

    #[test]
    fn a_damaged_file_is_ignored() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(FILE), b"TLSI garbage").unwrap();
        assert!(load(d.path(), &fingerprint("m", &[])).is_none());
    }
}
