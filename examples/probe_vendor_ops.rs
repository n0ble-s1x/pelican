//! Probe each Garmin vendor op (0x9000-0x900B) with no params, see what it
//! returns. Read-only intent — we expect ParameterNotSupported / OK / data.

use mtp::MtpDevice;
use mtp::OperationCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let session = device.session();

    for code in 0x9000u16..=0x900Bu16 {
        let op = OperationCode::Unknown(code);
        eprint!("op 0x{:04X}: ", code);
        match session.execute(op, &[]).await {
            Ok(resp) => eprintln!(
                "OK code={:?} params={:?}",
                resp.code, resp.params
            ),
            Err(e) => eprintln!("Err: {e}"),
        }
    }
    Ok(())
}
