//! Print everything we can learn about the attached Garmin device.
//! Use this when uploads time out or behave oddly.
//!
//!   cargo run --example diagnose

use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    let info = device.device_info();
    println!("manufacturer: {}", info.manufacturer);
    println!("model:        {}", info.model);
    println!("device_version: {}", info.device_version);
    println!("serial:       {}", info.serial_number);
    println!("vendor_ext:   {}", info.vendor_extension_desc);
    println!();

    let storages = device.storages().await?;
    println!("storages: {}", storages.len());
    for (i, s) in storages.iter().enumerate() {
        let si = s.info();
        println!(
            "  [{i}] id={:08X} desc={:?} type={:?} fs={:?} access={:?} max={} free={}",
            s.id().0,
            si.description,
            si.storage_type,
            si.filesystem_type,
            si.access_capability,
            si.max_capacity,
            si.free_space_bytes,
        );
    }
    println!();

    if let Some(storage) = storages.first() {
        println!("=== root listing on storage[0] ===");
        let root = storage.list_objects(None).await?;
        for o in &root {
            let kind = if o.is_folder() { "DIR " } else { "FILE" };
            println!("  {kind} {} ({} bytes)", o.filename, o.size);
        }
        println!();

        // Walk EVERY top-level folder, streaming so broken entries don't
        // kill the listing. We're hunting for where the indexed music goes.
        for top in root.iter().filter(|o| o.is_folder()) {
            println!("=== /{} (streaming, skip errors) ===", top.filename);
            let mut stream = storage.list_objects_stream(Some(top.handle)).await?;
            let total = stream.total();
            let mut shown = 0usize;
            let mut errors = 0usize;
            while let Some(r) = stream.next().await {
                match r {
                    Ok(info) => {
                        if shown < 25 {
                            let kind = if info.is_folder() { "DIR " } else { "FILE" };
                            println!("  {kind} {} ({} bytes)", info.filename, info.size);
                        }
                        shown += 1;
                    }
                    Err(_) => errors += 1,
                }
            }
            if shown > 25 {
                println!("  … and {} more", shown - 25);
            }
            println!("  total handles = {total}, readable = {shown}, errored = {errors}");
            println!();

            // One level deeper for any subfolders we just listed (only first
            // 5 to keep output bounded).
            // We have to re-list because we didn't keep ObjectInfo.
            let infos = storage.list_objects(Some(top.handle)).await.unwrap_or_default();
            for sub in infos.iter().filter(|o| o.is_folder()).take(5) {
                println!("  --- /{}/{} ---", top.filename, sub.filename);
                let mut sub_stream =
                    storage.list_objects_stream(Some(sub.handle)).await?;
                let mut sub_shown = 0;
                while let Some(r) = sub_stream.next().await {
                    if let Ok(info) = r {
                        if sub_shown < 10 {
                            let kind = if info.is_folder() { "DIR " } else { "FILE" };
                            println!(
                                "    {kind} {} ({} bytes)",
                                info.filename, info.size
                            );
                        }
                        sub_shown += 1;
                    }
                }
                if sub_shown > 10 {
                    println!("    … and {} more", sub_shown - 10);
                }
            }
            println!();
        }
    }

    Ok(())
}
