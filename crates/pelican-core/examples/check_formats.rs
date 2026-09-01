use mtp::MtpDevice;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    let info = device.device_info();
    println!("vendor_ext: {}", info.vendor_extension_desc);
    println!(
        "operations_supported ({}):",
        info.operations_supported.len()
    );
    for op in &info.operations_supported {
        println!("  {:?}", op);
    }
    println!("playback_formats ({}):", info.playback_formats.len());
    for f in &info.playback_formats {
        println!("  {:?}", f);
    }
    println!("capture_formats ({}):", info.capture_formats.len());
    Ok(())
}
