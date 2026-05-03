//! Probe each Garmin vendor op (0x9000-0x900B + 0x9810/0x9811) with no params.
//!
//! Wraps each call in `tokio::time::timeout` so a single hung op doesn't kill
//! the whole sweep. Flushes stdout per line so progress is visible live.

use std::io::Write;
use std::time::Duration;

use mtp::{MtpDevice, OperationCode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let session = device.session();

    let codes: Vec<u16> = (0x9000u16..=0x900B).chain([0x9810u16, 0x9811]).collect();
    for code in codes {
        let op = OperationCode::Unknown(code);
        let s = format!("op 0x{code:04X}: ");
        print!("{s}");
        std::io::stdout().flush().ok();
        let res = tokio::time::timeout(Duration::from_secs(3), session.execute(op, &[])).await;
        match res {
            Ok(Ok(resp)) => println!("OK code={:?} params={:?}", resp.code, resp.params),
            Ok(Err(e)) => println!("Err: {e}"),
            Err(_) => println!("TIMEOUT (3s)"),
        }
    }
    Ok(())
}
