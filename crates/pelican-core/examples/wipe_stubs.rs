//! Find and delete every unreadable handle in /Music — broken-stub
//! cleanup. Leaves readable files alone. Safer than `wipe_music`.
//!
//! SCOPE, and the name overstates it: this removes objects from `/Music`.
//! It does not clear the watch's music app. `docs/garmin-mtp.md` §8 records
//! 22 deletes that all returned `Ok`, a `/Music` that fell to one entry and
//! 78.5 MB of free space returned — while the watch went on listing every
//! track. Do not reach for this as a reset and conclude it failed silently.

use anyhow::Result;
use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
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

    let session = device.session();
    let handles = session
        .get_object_handles(storage.id(), None, Some(music.handle))
        .await?;
    println!("/Music has {} handles", handles.len());

    let mut readable = 0usize;
    let mut deleted = 0usize;
    let mut delete_failed = 0usize;
    for (idx, handle) in handles.iter().enumerate() {
        match session.get_object_info(*handle).await {
            Ok(_) => readable += 1,
            Err(_) => {
                print!("  unreadable #{} handle={handle:?} ... ", idx + 1);
                match storage.delete(*handle).await {
                    Ok(()) => {
                        println!("deleted");
                        deleted += 1;
                    }
                    Err(e) => {
                        println!("delete failed: {e}");
                        delete_failed += 1;
                    }
                }
            }
        }
    }
    println!("done: {readable} readable, {deleted} stubs deleted, {delete_failed} delete failures");
    Ok(())
}
