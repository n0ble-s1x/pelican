//! Read the data phase of Garmin vendor op 0x9000 (and 0x9001).
//! Read-only: no parameters, no writes.
use mtp::{MtpDevice, OperationCode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = MtpDevice::open_first().await?;
    dev.session().set_split_header_data(true);
    for code in [0x9000u16, 0x9001] {
        let op = OperationCode::Unknown(code);
        print!("0x{code:04X}  ");
        match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            dev.session().execute_with_receive(op, &[]),
        )
        .await
        {
            Ok(Ok((resp, bytes))) => {
                println!("resp={:?} params={:?}", resp.code, resp.params);
                println!("{} bytes", bytes.len());
                let show = &bytes[..bytes.len().min(256)];
                println!(
                    "   hex : {}",
                    show.iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                let ascii: String = show
                    .iter()
                    .map(|&b| {
                        if (0x20..0x7f).contains(&b) {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                println!("   ascii: {ascii}");
                // MTP strings are UTF-16LE; try that too
                if bytes.len() > 2 {
                    let u: Vec<u16> = bytes[1..]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    let s = String::from_utf16_lossy(&u);
                    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
                    if clean.chars().filter(|c| c.is_alphanumeric()).count() > 3 {
                        println!("   utf16: {}", &clean[..clean.len().min(200)]);
                    }
                }
            }
            Ok(Err(e)) => println!("err: {e}"),
            Err(_) => println!("timeout"),
        }
        println!();
    }
    Ok(())
}
