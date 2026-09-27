//! Download a single file from /Music and hex-dump its bytes.
//! Usage: cargo run --release --example dump_file -- <filename>

use anyhow::Result;
use mtp::MtpDevice;
// The one fold Pelican uses for every device name, so `CAFÉ.mp3` finds
// `café.mp3` here exactly as it does in a run.
use pelican_core::mtp::same_file;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = args.first().map(|s| s.as_str()).unwrap_or("PelicanF.m3u8");

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
        .find(|o| o.is_folder() && same_file(&o.filename, "Music"))
        .ok_or_else(|| anyhow::anyhow!("no Music folder"))?;
    let entries = storage.list_objects(Some(music.handle)).await?;
    let target = entries
        .into_iter()
        .find(|o| !o.is_folder() && same_file(&o.filename, name))
        .ok_or_else(|| anyhow::anyhow!("file not found: {name}"))?;

    println!(
        "found {name} handle={:?} size={}",
        target.handle, target.size
    );
    let bytes = storage.download(target.handle).await?;
    println!("downloaded {} bytes", bytes.len());
    println!("--- raw bytes (escaped) ---");
    for chunk in bytes.chunks(48) {
        let hex: String = chunk.iter().map(|b| format!("{b:02x} ")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("  {hex:<144} | {ascii}");
    }
    Ok(())
}
