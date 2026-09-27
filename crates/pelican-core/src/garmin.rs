use anyhow::{anyhow, Context, Result};
use nusb::MaybeFuture;

pub const GARMIN_VENDOR_ID: u16 = 0x091E;

/// The product id a Garmin watch shows while it is not speaking MTP — seen
/// on a Forerunner 165 right after a reboot, as "Garmin GPS usb/tty
/// converter". A watch stuck there answers no MTP request until replugged.
pub const SERIAL_MODE_PRODUCT_ID: u16 = 0x0003;

/// Top-level folder on the watch where playable music lives.
pub const MUSIC_FOLDER: &str = "Music";

#[derive(Debug, Clone)]
pub struct Device {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub product: Option<String>,
}

impl Device {
    /// Human-readable device label.
    ///
    /// Product and serial come from USB descriptors the device controls, and
    /// this string is printed to the terminal, so control bytes are stripped
    /// at the source rather than at each call site.
    pub fn label(&self) -> String {
        let name = strip_control(self.product.as_deref().unwrap_or("Garmin device"));
        match &self.serial {
            Some(s) => format!("{name} ({})", strip_control(s)),
            None => name,
        }
    }
}

/// Drop control characters from device-controlled text before it is printed.
///
/// USB descriptor strings, MTP model names and object filenames all come off
/// the watch, and a terminal will act on an escape sequence hidden in any of
/// them. Everything the device says passes through here on its way out.
pub fn strip_control(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
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
    choose(list_devices()?, serial)
}

/// [`pick_device`]'s decision, on a given list. Pure, for tests.
pub fn choose(devices: Vec<Device>, serial: Option<&str>) -> Result<Device> {
    let (devices, stuck): (Vec<Device>, Vec<Device>) = devices
        .into_iter()
        .partition(|d| d.product_id != SERIAL_MODE_PRODUCT_ID);
    if devices.is_empty() && !stuck.is_empty() {
        return Err(anyhow::Error::new(crate::error::Wedged).context(
            "a Garmin watch is on USB but not in MTP mode (it shows up as a serial converter, \
             which happens after the watch reboots)",
        ));
    }
    if devices.is_empty() {
        return Err(anyhow!(
            "no Garmin device found on USB (vendor 0x{GARMIN_VENDOR_ID:04x}). \
             Plug in your watch, unlock it, and check that USB mode is set to MTP."
        ));
    }
    if let Some(want) = serial {
        if stuck.iter().any(|d| d.serial.as_deref() == Some(want))
            && !devices.iter().any(|d| d.serial.as_deref() == Some(want))
        {
            return Err(anyhow::Error::new(crate::error::Wedged)
                .context(format!("watch {want} is on USB but not in MTP mode")));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(pid: u16, serial: &str) -> Device {
        Device {
            vendor_id: GARMIN_VENDOR_ID,
            product_id: pid,
            serial: Some(serial.into()),
            product: None,
        }
    }

    #[test]
    fn a_watch_stuck_in_serial_mode_is_a_wedge() {
        let e = choose(vec![dev(SERIAL_MODE_PRODUCT_ID, "1")], None).unwrap_err();
        assert!(crate::error::is_wedged(&e), "{e:#}");
        assert!(format!("{e:#}").contains(crate::error::REPLUG));
        // An MTP-mode watch beside it is still picked.
        let d = choose(
            vec![dev(SERIAL_MODE_PRODUCT_ID, "1"), dev(0x4f0b, "2")],
            None,
        )
        .unwrap();
        assert_eq!(d.serial.as_deref(), Some("2"));
        let e = choose(Vec::new(), None).unwrap_err();
        assert_eq!(
            crate::error::classify(&e),
            crate::error::DeviceErrorKind::NotFound
        );
    }

    #[test]
    fn label_strips_escape_sequences_from_descriptor_strings() {
        let d = Device {
            vendor_id: GARMIN_VENDOR_ID,
            product_id: 0x5151,
            serial: Some("12\u{1b}[2J34".into()),
            product: Some("Fore\u{7}runner".into()),
        };
        let label = d.label();
        assert!(!label.chars().any(char::is_control), "{label:?}");
        assert_eq!(label, "Forerunner (12[2J34)");
    }
}
