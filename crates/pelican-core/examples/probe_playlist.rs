//! Try a series of playlist write variants against the watch and report
//! which (if any) actually persist as readable playlist files.
//!
//! Each variant is attempted in its own MTP session (open + write + close).
//! Between attempts we re-list `/Music` to see whether the previous write
//! produced a readable playlist or just a broken stub. We DO NOT delete
//! stubs between attempts — they don't seem to interfere and the watch
//! eventually GCs them.
//!
//! Each variant writes to a distinct filename so we can tell which one (if
//! any) appeared in the music app.

use anyhow::Result;
use bytes::Bytes;
use mtp::{MtpDevice, NewObjectInfo, ObjectFormatCode};

#[derive(Clone)]
struct Variant {
    label: &'static str,
    filename: &'static str,
    format_code: u16,
    body: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Two real tracks already on the watch. If you change these, change them
    // both above and below.
    let track_a = "repeat-1.mp3";
    let track_b = "repeat-2.mp3";

    let variants = vec![
        Variant {
            label: "A: bare basenames, format 0xBA05",
            filename: "PelicanA.m3u8",
            format_code: 0xBA05,
            body: format!("#EXTM3U\n{track_a}\n{track_b}\n"),
        },
        Variant {
            label: "B: uppercase 0:/MUSIC/, format 0xBA05",
            filename: "PelicanB.m3u8",
            format_code: 0xBA05,
            body: format!(
                "#EXTM3U\n0:/MUSIC/{a}\n0:/MUSIC/{b}\n",
                a = track_a.to_ascii_uppercase(),
                b = track_b.to_ascii_uppercase()
            ),
        },
        Variant {
            label: "C: case-preserving 0:/Music/, format 0xBA05",
            filename: "PelicanC.m3u8",
            format_code: 0xBA05,
            body: format!("#EXTM3U\n0:/Music/{track_a}\n0:/Music/{track_b}\n"),
        },
        Variant {
            label: "D: bare basenames, format 0xBA10 (AbstractAudioPlaylist)",
            filename: "PelicanD.m3u8",
            format_code: 0xBA10,
            body: format!("#EXTM3U\n{track_a}\n{track_b}\n"),
        },
        Variant {
            label: "E: backslash-style /Music\\track, format 0xBA05",
            filename: "PelicanE.m3u8",
            format_code: 0xBA05,
            body: format!("#EXTM3U\nMusic\\{track_a}\nMusic\\{track_b}\n"),
        },
        Variant {
            label: "F: with #EXTINF lines, bare basenames, format 0xBA05",
            filename: "PelicanF.m3u8",
            format_code: 0xBA05,
            body: format!(
                "#EXTM3U\n#EXTINF:-1,Track A\n{track_a}\n#EXTINF:-1,Track B\n{track_b}\n"
            ),
        },
    ];

    for v in &variants {
        println!("\n=== {} ===", v.label);
        match try_variant(v).await {
            Ok(info) => println!("  write: {info}"),
            Err(e) => println!("  ERROR: {e:#}"),
        }
        // Brief settle so the watch finishes its post-write validation
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match check_playlist(v.filename).await {
            Ok(found) => {
                if found.is_some() {
                    println!("  RESULT: ✓ playlist visible — {:?}", found);
                } else {
                    println!("  RESULT: ✗ not visible (silent reject / broken stub)");
                }
            }
            Err(e) => println!("  check error: {e:#}"),
        }
    }

    Ok(())
}

async fn try_variant(v: &Variant) -> Result<String> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let storages = device.storages().await?;
    let storage = storages
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no storage"))?;

    // Find Music folder (case-insensitive)
    let root = storage.list_objects(None).await?;
    let music = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Music"))
        .ok_or_else(|| anyhow::anyhow!("no Music folder at root"))?;

    let body_bytes = v.body.as_bytes();
    let info = NewObjectInfo::with_format(
        v.filename,
        body_bytes.len() as u64,
        ObjectFormatCode::Unknown(v.format_code),
    );
    let chunks = futures::stream::iter(vec![Ok::<_, std::io::Error>(Bytes::copy_from_slice(
        body_bytes,
    ))]);
    storage
        .upload(Some(music.handle), info, Box::pin(chunks))
        .await?;
    Ok(format!(
        "{} bytes, format=0x{:04X}",
        body_bytes.len(),
        v.format_code
    ))
}

async fn check_playlist(name: &str) -> Result<Option<u64>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let storages = device.storages().await?;
    let storage = storages
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no storage"))?;
    let root = storage.list_objects(None).await?;
    let music = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Music"))
        .ok_or_else(|| anyhow::anyhow!("no Music folder"))?;
    let entries = storage
        .list_objects(Some(music.handle))
        .await
        .unwrap_or_default();
    Ok(entries
        .into_iter()
        .find(|o| !o.is_folder() && o.filename.eq_ignore_ascii_case(name))
        .map(|o| o.size))
}
