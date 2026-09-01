//! macOS: is anything holding the watch?
//!
//! The suspect is Apple's `ptpcamerad`, a LaunchAgent that claims
//! still-image-class USB devices on attach and is SIP-protected against
//! being signalled. It genuinely does block Android phones and cameras.
//!
//! **It does not claim Garmin watches.** Verified 2026-08-30 against a
//! Forerunner 165 Music (firmware 2506) on macOS 26.6.2: with `ptpcamerad`
//! running, the watch attached as `091e:5151` carrying no `UsbExclusiveOwner`
//! at all, and Pelican opened an MTP session, listed `/Music`, uploaded,
//! verified and deleted without touching it. The watch reports
//! `bDeviceClass = 0` and its MTP interface is not still-image class, so
//! `ptpcamerad` never matches it.
//!
//! That is why this module reports contention **only on real evidence** — an
//! actual `UsbExclusiveOwner` on a Garmin node. An earlier version treated
//! "ptpcamerad is running" as proof of a problem, which fired on every Mac
//! including ones working perfectly. A warning that is always on is a
//! warning nobody reads.
//!
//! When an open *does* fail for exclusive access, [`explain_exclusive_access`]
//! turns it into something actionable — and the first thing it names is our
//! own still-open session, which is by far the most common cause.
//!
//! Everything here shells out to `ioreg` / `pgrep` rather than linking IOKit,
//! which keeps `unsafe_code = "deny"` intact across the whole crate.

use std::process::Command;

use super::Contention;
use crate::garmin::GARMIN_VENDOR_ID;

/// The command that actually frees the device, for the current user.
fn remedy() -> String {
    // `id -u` rather than a baked uid: the plist lives in the per-user GUI
    // domain, and hard-coding 501 would be wrong on any other account.
    "sudo launchctl disable gui/$(id -u)/com.apple.ptpcamerad \\\n    \
     && sudo launchctl bootout gui/$(id -u)/com.apple.ptpcamerad"
        .to_string()
}

/// Ask the IORegistry who holds the Garmin device, if anyone.
///
/// Returns the raw `UsbExclusiveOwner` value — a process name string that
/// the OS wrote, not the device, but it is still treated as untrusted text
/// and stripped of control bytes before it can reach a terminal.
fn exclusive_owner() -> Option<String> {
    let out = Command::new("ioreg")
        .args(["-p", "IOUSB", "-l", "-w", "0"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);

    // ioreg prints idVendor in decimal. Walk the tree and remember whether
    // the node we are inside belongs to Garmin, so we only report an owner
    // for *our* device and not for someone's webcam.
    let vendor_needle = format!("\"idVendor\" = {}", GARMIN_VENDOR_ID);
    let mut in_garmin = false;
    for line in text.lines() {
        if line.contains(&vendor_needle) {
            in_garmin = true;
        } else if line.contains("\"idVendor\" = ") {
            in_garmin = false;
        }
        if in_garmin {
            if let Some(rest) = line.split("\"UsbExclusiveOwner\" = ").nth(1) {
                let owner = rest.trim().trim_matches('"');
                if !owner.is_empty() {
                    return Some(crate::playlist::strip_control(owner));
                }
            }
        }
    }
    None
}

fn ptpcamerad_running() -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg("ptpcamerad")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns `Some` only when something actually holds a Garmin device.
///
/// The single accepted piece of evidence is a `UsbExclusiveOwner` on a
/// Garmin node in the IORegistry. The mere presence of `ptpcamerad` is not
/// evidence — see the module header; it runs on every Mac and leaves Garmin
/// watches alone.
pub fn detect() -> Option<Contention> {
    let owner = exclusive_owner()?;
    Some(Contention {
        holder: owner.clone(),
        detail: format!("IORegistry UsbExclusiveOwner = {owner}"),
        remedy: if owner.contains("ptpcamerad") {
            remedy()
        } else {
            format!("quit {owner}, then re-run Pelican")
        },
        // Even when the holder is ptpcamerad we cannot signal it: verified
        // on macOS 26.6.2 that a same-user `killall` exits 0 and the process
        // survives with its PID unchanged.
        self_fixable: false,
    })
}

/// Turn an exclusive-access failure into advice, in likelihood order.
///
/// Called only once an open has actually failed, so it can afford to name
/// possibilities the passive [`detect`] must stay silent about.
pub fn explain_exclusive_access() -> String {
    let mut causes = vec![
        "another Pelican session is still open — the watch allows one at a \
         time, so close any other window or finish the transfer in flight"
            .to_string(),
    ];
    if let Some(owner) = exclusive_owner() {
        causes.push(format!("{owner} holds the device"));
    } else if ptpcamerad_running() {
        // Worth mentioning last: it is running on every Mac and does not
        // claim Garmin hardware, but it does claim phones and cameras, so a
        // user syncing something else may genuinely be hitting it.
        causes.push(format!(
            "Apple's ptpcamerad is running. It does not claim Garmin watches, \
             but if you are syncing another device it may hold it:\n    {}",
            remedy()
        ));
    }
    causes.push(
        "the device was unplugged and replugged very recently — macOS takes \
         a moment to release the interface"
            .to_string(),
    );
    let mut out = String::from("could not get exclusive access to the watch. Likely causes:");
    for (i, c) in causes.iter().enumerate() {
        out.push_str(&format!("\n  {}. {c}", i + 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remedy_is_per_user_not_hardcoded_uid() {
        // A baked 501 silently does nothing on any other account.
        let r = remedy();
        assert!(r.contains("$(id -u)"), "remedy hardcodes a uid: {r}");
        assert!(!r.contains("gui/501"), "remedy hardcodes uid 501: {r}");
    }

    #[test]
    fn a_running_ptpcamerad_is_not_by_itself_contention() {
        // Regression guard for a real false positive. ptpcamerad runs on
        // every Mac and does not claim Garmin watches (verified against an
        // FR165 on macOS 26.6.2), so detect() must stay silent unless the
        // IORegistry actually names an owner. Firing here would put a
        // permanent scary banner in front of every macOS user.
        if ptpcamerad_running() && exclusive_owner().is_none() {
            assert!(
                detect().is_none(),
                "detect() fired with no UsbExclusiveOwner — false positive is back"
            );
        }
    }

    #[test]
    fn contention_is_never_advertised_as_self_fixable() {
        // Verified on macOS 26.6.2: killall returns 0 and the process lives.
        if let Some(c) = detect() {
            assert!(!c.self_fixable);
        }
    }

    #[test]
    fn exclusive_access_advice_leads_with_our_own_session() {
        // The overwhelmingly common cause, and the only one the user can act
        // on immediately. Verified during bring-up: a probe holding its own
        // listing session made every upload fail this way.
        let msg = explain_exclusive_access();
        let first = msg.lines().nth(1).unwrap_or_default();
        assert!(
            first.contains("another Pelican session"),
            "advice should lead with the likeliest cause, got: {first}"
        );
    }

    #[test]
    fn detect_does_not_panic() {
        let _ = detect();
        let _ = explain_exclusive_access();
    }
}
