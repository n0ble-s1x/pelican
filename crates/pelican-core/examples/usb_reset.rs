//! Send a USB device reset to the Garmin. Sometimes wakes a watch that's
//! stuck post-power-cycle without requiring a physical replug.

use nusb::MaybeFuture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infos = nusb::list_devices().wait()?;
    for info in infos {
        if info.vendor_id() == 0x091e {
            println!(
                "found Garmin {:04x}:{:04x} serial={:?}",
                info.vendor_id(),
                info.product_id(),
                info.serial_number()
            );
            let dev = info.open().wait()?;
            dev.reset().wait()?;
            println!("issued USB reset");
            return Ok(());
        }
    }
    println!("no Garmin device found");
    Ok(())
}
