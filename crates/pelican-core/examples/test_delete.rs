//! Diagnostic: list /Music and attempt DeleteObject on every handle.
//! Surfaces whether Garmin's DeleteObject works at all on this storage.
//!
//! SCOPE, and the name overstates it: this removes objects from `/Music`.
//! It does not clear the watch's music app. `docs/garmin-mtp.md` §8 records
//! 22 deletes that all returned `Ok`, a `/Music` that fell to one entry and
//! 78.5 MB of free space returned — while the watch went on listing every
//! track. Do not reach for this as a reset and conclude it failed silently.

use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let storages = device.storages().await?;
    let storage = &storages[0];
    let storage_id = storage.id();
    let session = device.session();

    let root = storage.list_objects(None).await?;
    let music = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Music"))
        .ok_or("no Music/")?;

    let handles = session
        .get_object_handles(storage_id, None, Some(music.handle))
        .await?;
    println!("/Music has {} handles", handles.len());

    for h in &handles {
        let info = session.get_object_info(*h).await;
        let name = match &info {
            Ok(i) => i.filename.clone(),
            Err(_) => format!("‹unreadable #{}›", h.0),
        };
        print!("  delete {} (handle {:?}) ... ", name, h);
        match storage.delete(*h).await {
            Ok(()) => println!("OK"),
            Err(e) => println!("FAIL: {e}"),
        }
    }

    let after = session
        .get_object_handles(storage_id, None, Some(music.handle))
        .await?;
    println!("/Music after: {} handles", after.len());

    Ok(())
}
