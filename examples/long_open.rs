//! Try opening with a 90-second timeout in case the watch firmware needs
//! more time to respond to OpenSession after a power-cycle.

use mtp::MtpDevice;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("opening with 90s timeout…");
    let device = MtpDevice::builder()
        .timeout(Duration::from_secs(90))
        .open_first()
        .await?;
    println!(
        "open OK: {} {}",
        device.device_info().manufacturer,
        device.device_info().model
    );
    Ok(())
}
