use anyhow::{anyhow, Context, Result};
use nusb::MaybeFuture;

pub const GARMIN_VENDOR_ID: u16 = 0x091E;

/// Top-level folder on the watch where playable music lives.
pub const MUSIC_FOLDER: &str = "Music";

#[derive(Debug, Clone)]
#[allow(dead_code)] // vendor_id/product_id surfaced for future "known model" warnings
pub struct Device {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub product: Option<String>,
}

impl Device {
    pub fn label(&self) -> String {
        let name = self.product.as_deref().unwrap_or("Garmin device");
        match &self.serial {
            Some(s) => format!("{name} ({s})"),
            None => name.to_string(),
        }
    }
}

pub fn list_devices() -> Result<Vec<Device>> {
    let mut out = Vec::new();
    let infos = nusb::list_devices()
        .wait()
        .context("enumerating USB devices")?;
    for info in infos {
        if info.vendor_id() != GARMIN_VENDOR_ID {
            continue;
        }
        out.push(Device {
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            serial: info.serial_number().map(str::to_owned),
            product: info.product_string().map(str::to_owned),
        });
    }
    Ok(out)
}

pub fn pick_device(serial: Option<&str>) -> Result<Device> {
    let devices = list_devices()?;
    if devices.is_empty() {
        return Err(anyhow!(
            "no Garmin device found on USB (vendor 0x{GARMIN_VENDOR_ID:04x}). \
             Plug in your watch, unlock it, and check that USB mode is set to MTP."
        ));
    }
    if let Some(want) = serial {
        return devices
            .into_iter()
            .find(|d| d.serial.as_deref() == Some(want))
            .ok_or_else(|| anyhow!("no Garmin device with serial {want}"));
    }
    if devices.len() == 1 {
        return Ok(devices.into_iter().next().unwrap());
    }
    let mut msg = String::from("multiple Garmin devices found. Pick one with --serial:\n");
    for d in &devices {
        msg.push_str(&format!("  - {}\n", d.label()));
    }
    Err(anyhow!(msg))
}
