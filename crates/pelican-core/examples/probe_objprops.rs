//! Can the watch actually report tags over MTP?
//!
//! Pelican itself never asks: it writes its own tags and proves the bytes.
//! Read-only: no writes, no deletes.
use mtp::ptp::ObjectPropertyCode;
use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = MtpDevice::open_first().await?;
    dev.session().set_split_header_data(true);

    let ops = &dev.device_info().operations_supported;
    let want = [0x9801u16, 0x9802, 0x9803, 0x9804, 0x9805];
    println!("operations_supported: {} total", ops.len());
    for w in want {
        let has = ops.iter().any(|o| u16::from(*o) == w);
        println!(
            "  0x{w:04x} {:<22} {}",
            match w {
                0x9801 => "GetObjectPropsSupported",
                0x9802 => "GetObjectPropDesc",
                0x9803 => "GetObjectPropValue",
                0x9804 => "SetObjectPropValue",
                _ => "GetObjectPropList",
            },
            if has { "YES" } else { "no" }
        );
    }

    let storage = dev
        .storages()
        .await?
        .into_iter()
        .next()
        .ok_or("no storage")?;
    let sid = storage.id();
    let mut music = None;
    let mut stream = storage.list_objects_stream(None).await?;
    while let Some(Ok(i)) = stream.next().await {
        if i.is_folder() && i.filename == "Music" {
            music = Some(i.handle);
            break;
        }
    }
    drop(stream);
    let music = music.ok_or("no /Music")?;

    let handles = dev
        .session()
        .get_object_handles(sid, None, Some(music))
        .await?;
    println!(
        "\n/Music: {} handles, reading properties for the first 4\n",
        handles.len()
    );

    for h in handles.iter().take(4) {
        let name = dev
            .session()
            .get_object_info(*h)
            .await
            .map(|i| i.filename)
            .unwrap_or_default();
        println!("  {name}");
        for (label, code) in [
            ("Name", ObjectPropertyCode::Name),
            ("Artist", ObjectPropertyCode::Unknown(0xDC46)),
            ("AlbumName", ObjectPropertyCode::Unknown(0xDC9A)),
            ("AlbumArtist", ObjectPropertyCode::Unknown(0xDC9B)),
            ("Duration", ObjectPropertyCode::Unknown(0xDC89)),
            ("Track", ObjectPropertyCode::Unknown(0xDC8B)),
        ] {
            match dev.session().get_object_prop_value(*h, code).await {
                Ok(v) => println!("      {label:<12} {v:?}"),
                Err(e) => println!("      {label:<12} <error: {e}>"),
            }
        }
        println!();
    }
    Ok(())
}
