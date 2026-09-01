use bytes::Bytes;
use mtp::{MtpDevice, NewObjectInfo};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let storages = device.storages().await?;
    let storage = &storages[0];

    // free before
    let before = storages[0].info().free_space_bytes;
    eprintln!("free before: {}", before);

    let root = storage.list_objects(None).await?;
    let target = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Audiobooks"))
        .ok_or("no audiobooks")?;
    let bytes = std::fs::read("/tmp/probe.mp3")?;
    let info = NewObjectInfo::file("probe.mp3", bytes.len() as u64);
    let stream = futures::stream::iter(
        bytes
            .chunks(256 * 1024)
            .map(|c| Ok::<_, std::io::Error>(Bytes::copy_from_slice(c)))
            .collect::<Vec<_>>(),
    );
    let h = storage
        .upload(Some(target.handle), info, Box::pin(stream))
        .await?;
    eprintln!("uploaded handle: {:?}", h);

    // re-fetch storages
    let storages2 = device.storages().await?;
    let after = storages2[0].info().free_space_bytes;
    eprintln!(
        "free after: {}, delta: {}",
        after,
        before as i64 - after as i64
    );

    // List /Audiobooks contents
    let session = device.session();
    let storage_id = storage.id();
    let handles = session
        .get_object_handles(storage_id, None, Some(target.handle))
        .await?;
    eprintln!("/Audiobooks now has {} handles", handles.len());
    for h in &handles {
        match session.get_object_info(*h).await {
            Ok(info) => eprintln!("  FILE {} ({} bytes)", info.filename, info.size),
            Err(e) => eprintln!("  unreadable #{}: {}", h.0, e),
        }
    }
    Ok(())
}
