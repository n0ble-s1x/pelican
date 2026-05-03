//! Resilient enumerate-and-delete of /Music on the watch.
//!
//! Uses `list_objects_stream` so we can skip individual `GetObjectInfo`
//! errors (broken stubs from prior partial uploads) and delete every entry
//! whose handle we can resolve.

use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let storages = device.storages().await?;
    let storage = &storages[0];
    println!(
        "Storage: {} · {} bytes free",
        storage.info().description,
        storage.info().free_space_bytes
    );

    let root = storage.list_objects(None).await?;
    let music = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Music"))
        .ok_or("no Music/ folder")?;

    let mut stream = storage.list_objects_stream(Some(music.handle)).await?;
    let total = stream.total();
    println!("Found {} entries in /Music — wiping", total);

    let mut handles = Vec::new();
    let mut failed_info = 0usize;
    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                println!(
                    "  found: {} ({} bytes) handle={:?}",
                    info.filename, info.size, info.handle
                );
                handles.push(info.handle);
            }
            Err(e) => {
                failed_info += 1;
                eprintln!("  GetObjectInfo failed: {e}");
            }
        }
    }
    println!("Got info on {}, failed on {}.", handles.len(), failed_info);

    // To clean up the broken stubs, we'd also need their handles — but
    // GetObjectInfo failed on them, so we don't have them. Let's still try
    // deleting every handle we DO have, which clears the readable entries.
    let mut deleted = 0usize;
    for h in handles {
        match storage.delete(h).await {
            Ok(()) => deleted += 1,
            Err(e) => eprintln!("  delete {h:?} failed: {e}"),
        }
    }
    println!("Deleted {deleted} entries.");

    // Re-fetch storage info for fresh free-space.
    let storages = device.storages().await?;
    println!(
        "After wipe: {} bytes free",
        storages[0].info().free_space_bytes
    );

    Ok(())
}
