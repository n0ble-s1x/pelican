//! Diagnostic: open the Garmin and claim interface 0 directly via nusb,
//! to isolate whether the "interface is busy" comes from open vs claim.

use nusb::MaybeFuture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infos = nusb::list_devices().wait()?;
    for info in infos {
        if info.vendor_id() == 0x091e {
            println!("found {:04x}:{:04x}", info.vendor_id(), info.product_id());
            println!("opening...");
            let dev = info.open().wait()?;
            println!("opened OK. claiming interface 0...");
            let _iface = dev.claim_interface(0).wait()?;
            println!("claimed OK. releasing.");
            return Ok(());
        }
    }
    println!("no Garmin found");
    Ok(())
}
