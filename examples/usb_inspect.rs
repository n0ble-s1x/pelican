//! Open the watch via nusb (low-level) without going through MTP. Reports
//! whether a basic USB control transfer round-trips, which tells us if the
//! watch is firmware-alive vs only USB-PHY-alive.

use nusb::MaybeFuture;
use nusb::transfer::{ControlIn, ControlType, Recipient};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infos = nusb::list_devices().wait()?;
    for info in infos {
        if info.vendor_id() == 0x091e {
            println!("found Garmin {:04x}:{:04x}", info.vendor_id(), info.product_id());
            let dev = info.open().wait()?;
            // Try to read the device descriptor — control transfer to EP0.
            // If this works, USB is alive. If it hangs, the watch firmware is asleep.
            let req = ControlIn {
                control_type: ControlType::Standard,
                recipient: Recipient::Device,
                request: 0x06,            // GET_DESCRIPTOR
                value: 0x0100,            // DEVICE descriptor
                index: 0,
                length: 18,
            };
            println!("issuing GET_DESCRIPTOR control read…");
            match dev.control_in(req, Duration::from_secs(5)).wait() {
                Ok(buf) => println!("  got {} bytes: {:02x?}", buf.len(), &buf[..buf.len().min(18)]),
                Err(e) => println!("  ERROR: {e}"),
            }
            // Also try claiming interface 0
            println!("claiming interface 0…");
            match dev.claim_interface(0).wait() {
                Ok(_iface) => println!("  claimed OK"),
                Err(e) => println!("  ERROR: {e}"),
            }
            return Ok(());
        }
    }
    Ok(())
}
