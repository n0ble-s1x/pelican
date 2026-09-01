//! Probe each Garmin vendor op (0x9000-0x900B + 0x9810/0x9811) with no params.
//!
//! Wraps each call in `tokio::time::timeout` so a single hung op doesn't kill
//! the whole sweep. Writes each result to stdout AND appends a line to
//! `target/probe_vendor_ops.log` so partial progress survives an abort.
//!
//! The log distinguishes:
//!
//! - `OK <hex>`      responder returned a normal Response container
//! - `DATA <hex>`    responder started a data phase (op returns data) — worth
//!   a follow-up with a typed reader
//! - `SHORT <hex>`   response container too small (suggests a real response
//!   but with an unexpected param shape)
//! - `TIMEOUT`       responder did not reply within the per-op deadline
//! - `ERR <text>`    any other failure
//!
//! The first three are interesting (the op exists / is partially supported);
//! TIMEOUT usually means "needs parameters we didn't supply".

use std::fs::OpenOptions;
use std::io::Write as _;
use std::time::Duration;

use mtp::{MtpDevice, OperationCode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_path = std::path::Path::new("target/probe_vendor_ops.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    writeln!(log, "--- session @ {} ---", chrono_like_now())?;
    log.flush().ok();

    let device = MtpDevice::open_first().await?;
    device.session().set_split_header_data(true);
    let session = device.session();

    let codes: Vec<u16> = (0x9000u16..=0x900B).chain([0x9810u16, 0x9811]).collect();
    for code in codes {
        let op = OperationCode::Unknown(code);
        let res = tokio::time::timeout(Duration::from_secs(5), session.execute(op, &[])).await;
        let line = match res {
            Ok(Ok(resp)) => format!(
                "OK    0x{code:04X}  code={:?} params={:?}",
                resp.code, resp.params
            ),
            Ok(Err(e)) => {
                let msg = e.to_string();
                if msg.contains("expected Response container type (3), got 2") {
                    format!("DATA  0x{code:04X}  (responder started a data phase)")
                } else if msg.contains("response container too small") {
                    format!("SHORT 0x{code:04X}  ({msg})")
                } else if msg.to_ascii_lowercase().contains("timed out") {
                    format!("TIMEOUT 0x{code:04X}  (5s)")
                } else {
                    format!("ERR   0x{code:04X}  {msg}")
                }
            }
            Err(_) => format!("TIMEOUT 0x{code:04X}  (5s outer)"),
        };
        println!("{line}");
        writeln!(log, "{line}").ok();
        log.flush().ok();
    }
    writeln!(log, "--- done ---").ok();
    Ok(())
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch={secs}")
}
