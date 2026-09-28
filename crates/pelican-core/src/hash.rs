//! SHA-256, the one digest Pelican trusts.
//!
//! Two jobs: the source hash is how the ledger recognizes a track it has
//! already proven onto the watch (R5), and the transcode hash compared with
//! the read-back is the proof itself (R6). Size-only checks pass a file whose bytes were damaged in flight.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `data`.
pub fn bytes(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Lowercase hex SHA-256 of a file, streamed so a 60 MB WAV is never
/// held in memory whole.
pub fn file(path: &Path) -> Result<String> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn hex(digest: &[u8]) -> String {
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        assert_eq!(
            bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn file_and_bytes_agree() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x");
        let data: Vec<u8> = (0..600_000u32).map(|i| i as u8).collect();
        std::fs::write(&p, &data).unwrap();
        assert_eq!(file(&p).unwrap(), bytes(&data));
    }
}
