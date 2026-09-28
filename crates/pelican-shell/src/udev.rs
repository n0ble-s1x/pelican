//! The one privileged thing the window can do: install the USB rule.
//!
//! The rule text is compiled in from `udev/70-garmin-mtp.rules`, the same
//! file the packages ship, so there is one source of truth. Installing it
//! runs exactly one fixed command through polkit ([`PKEXEC`] + [`SCRIPT`]),
//! with the rule written to the child's stdin: nothing the user or the
//! webview supplies is interpolated into it. Everything that decides what
//! happens is pure and tested here; the tests inject a [`Runner`] and never
//! invoke pkexec.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

/// The rule, as shipped.
pub const RULE: &str = include_str!("../../../udev/70-garmin-mtp.rules");

/// Where an administrator's rule goes; it overrides a packaged one.
pub const ETC_RULE: &str = "/etc/udev/rules.d/70-garmin-mtp.rules";
/// Where the AUR and .deb packages install the same file.
pub const LIB_RULE: &str = "/usr/lib/udev/rules.d/70-garmin-mtp.rules";

/// Absolute, so `$PATH` never chooses what runs as root.
pub const PKEXEC: &str = "/usr/bin/pkexec";
pub const SH: &str = "/bin/sh";

/// The whole privileged step. A constant: the rule arrives on stdin.
pub const SCRIPT: &str = "install -m 644 /dev/stdin /etc/udev/rules.d/70-garmin-mtp.rules \
&& udevadm control --reload \
&& udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e \
&& udevadm settle";

/// The command exactly as it is run, for the window to show.
pub fn command_line() -> String {
    format!("{PKEXEC} {SH} -c '{SCRIPT}'")
}

/// The rule's effective lines: what udev reads, without comments or blanks.
fn effective(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// The same install for a terminal, self-contained (no checkout needed):
/// the effective rule is piped in, so the result compares as current.
pub fn manual_command() -> String {
    let body = effective(RULE).join("\n");
    format!(
        "printf '%s\\n' '{body}' | sudo install -m 644 /dev/stdin {ETC_RULE} \
         && sudo udevadm control --reload \
         && sudo udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e"
    )
}

// ── where we are ─────────────────────────────────────────────────────────

/// Inside a Flatpak, `/etc` is the sandbox's and pkexec cannot reach the
/// host, so the window must not pretend to install anything.
pub fn in_flatpak(flatpak_id: Option<&str>, flatpak_info_exists: bool) -> bool {
    flatpak_id.is_some_and(|id| !id.is_empty()) || flatpak_info_exists
}

fn in_flatpak_here() -> bool {
    in_flatpak(
        std::env::var("FLATPAK_ID").ok().as_deref(),
        Path::new("/.flatpak-info").exists(),
    )
}

// ── status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleState {
    /// Installed and the same rule as the one compiled in.
    Current,
    /// A file of that name is installed, but its rule differs.
    Outdated,
    /// Neither location holds it.
    Missing,
    /// Inside a Flatpak: the host's rules cannot be seen from here.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleStatus {
    pub state: RuleState,
    /// The file that decided the state, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether the window can offer the install (pkexec present, not a Flatpak).
    pub can_install: bool,
    /// Why it cannot, in one sentence, when it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_not: Option<String>,
    /// The rule text that would be written.
    pub rule: String,
    /// The privileged command, exactly as it would run.
    pub command: String,
    /// The same, for a terminal.
    pub manual: String,
}

/// Compare what is installed with the compiled-in rule. udev lets a file in
/// `/etc` shadow one of the same name in `/usr/lib`, so `/etc` decides when
/// it exists. `read` returns `None` for a file that is not there.
pub fn compare(
    read: impl Fn(&Path) -> Option<String>,
    paths: &[&Path],
) -> (RuleState, Option<PathBuf>) {
    for p in paths {
        if let Some(text) = read(p) {
            let state = if effective(&text) == effective(RULE) {
                RuleState::Current
            } else {
                RuleState::Outdated
            };
            return (state, Some(p.to_path_buf()));
        }
    }
    (RuleState::Missing, None)
}

fn why_not(flatpak: bool, pkexec_exists: bool) -> Option<String> {
    if flatpak {
        Some(format!(
            "Pelican is running as a Flatpak, which cannot install system files. \
             Run this once in a terminal on the host instead: {}",
            manual_command()
        ))
    } else if !pkexec_exists {
        Some(format!(
            "{PKEXEC} is not installed, so the window cannot ask for your password. \
             Run this once in a terminal instead: {}",
            manual_command()
        ))
    } else {
        None
    }
}

pub fn status_with(
    flatpak: bool,
    pkexec_exists: bool,
    read: impl Fn(&Path) -> Option<String>,
) -> RuleStatus {
    let (state, path) = if flatpak {
        (RuleState::Unknown, None)
    } else {
        compare(read, &[Path::new(ETC_RULE), Path::new(LIB_RULE)])
    };
    let why_not = why_not(flatpak, pkexec_exists);
    RuleStatus {
        state,
        path: path.map(|p| p.to_string_lossy().into_owned()),
        can_install: why_not.is_none(),
        why_not,
        rule: RULE.to_string(),
        command: command_line(),
        manual: manual_command(),
    }
}

pub fn status() -> RuleStatus {
    status_with(in_flatpak_here(), Path::new(PKEXEC).exists(), |p| {
        std::fs::read_to_string(p).ok()
    })
}

// ── install ──────────────────────────────────────────────────────────────

/// What a finished child reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// `None` when it was killed by a signal.
    pub code: Option<i32>,
    pub stderr: String,
}

/// Runs a program with fixed arguments and the given stdin.
pub trait Runner {
    fn run(&self, program: &str, args: &[&str], stdin: &[u8]) -> io::Result<Ran>;
}

/// The real one: no shell of ours, no `$PATH` lookup (the program is absolute).
pub struct System;

impl Runner for System {
    fn run(&self, program: &str, args: &[&str], stdin: &[u8]) -> io::Result<Ran> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        // Taken and dropped here, so the child sees end of input.
        if let Some(mut input) = child.stdin.take() {
            // A child that fails before reading (a dismissed prompt) closes
            // the pipe; its exit status is the answer, not this write.
            let _ = input.write_all(stdin);
        }
        let out = child.wait_with_output()?;
        Ok(Ran {
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Installed,
    /// The password prompt was dismissed or authorization was refused.
    Canceled,
    /// The window cannot install here (Flatpak, no pkexec).
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallResult {
    pub outcome: Outcome,
    pub message: String,
}

/// The last few non-empty lines of stderr, joined: enough to name the
/// problem without a wall of text.
fn summary(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    let s = tail.join(" ");
    if s.chars().count() > 300 {
        let cut: String = s.chars().take(300).collect();
        format!("{cut}…")
    } else {
        s
    }
}

/// Map what pkexec reported. pkexec exits 126 when the dialog is dismissed
/// and 127 when authorization is refused or cannot be obtained; `sh` uses
/// 127 too, for a command it cannot find, so a 127 whose stderr is not
/// pkexec's own is reported as the failure it is.
pub fn outcome(ran: &Ran) -> InstallResult {
    let canceled = || InstallResult {
        outcome: Outcome::Canceled,
        message: "Not installed: you canceled the password prompt.".into(),
    };
    let err = ran.stderr.to_ascii_lowercase();
    match ran.code {
        Some(0) => InstallResult {
            outcome: Outcome::Installed,
            message: format!("Installed {ETC_RULE} and reloaded udev."),
        },
        Some(126) => canceled(),
        Some(127) if err.contains("no authentication agent") => InstallResult {
            outcome: Outcome::Failed,
            message: format!(
                "Not installed: no password prompt could be shown (no polkit agent is running). \
                 Run this once in a terminal instead: {}",
                manual_command()
            ),
        },
        Some(127)
            if err.trim().is_empty()
                || err.contains("not authorized")
                || err.contains("dismissed") =>
        {
            canceled()
        }
        code => {
            let why = summary(&ran.stderr);
            let how = match code {
                Some(c) => format!("exit {c}"),
                None => "killed by a signal".into(),
            };
            InstallResult {
                outcome: Outcome::Failed,
                message: if why.is_empty() {
                    format!("Not installed: the install command failed ({how}).")
                } else {
                    format!("Not installed: {why} ({how}).")
                },
            }
        }
    }
}

pub fn install_with(flatpak: bool, pkexec_exists: bool, runner: &dyn Runner) -> InstallResult {
    if let Some(why) = why_not(flatpak, pkexec_exists) {
        return InstallResult {
            outcome: Outcome::Unavailable,
            message: why,
        };
    }
    match runner.run(PKEXEC, &[SH, "-c", SCRIPT], RULE.as_bytes()) {
        Ok(ran) => outcome(&ran),
        Err(e) => InstallResult {
            outcome: Outcome::Failed,
            message: format!("Not installed: could not start {PKEXEC}: {e}."),
        },
    }
}

pub fn install() -> InstallResult {
    install_with(in_flatpak_here(), Path::new(PKEXEC).exists(), &System)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A call the fake saw: program, arguments, stdin.
    type Call = (String, Vec<String>, Vec<u8>);

    struct Fake {
        ran: Ran,
        seen: RefCell<Vec<Call>>,
    }

    impl Fake {
        fn new(code: Option<i32>, stderr: &str) -> Self {
            Self {
                ran: Ran {
                    code,
                    stderr: stderr.into(),
                },
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Runner for Fake {
        fn run(&self, program: &str, args: &[&str], stdin: &[u8]) -> io::Result<Ran> {
            self.seen.borrow_mut().push((
                program.into(),
                args.iter().map(|a| a.to_string()).collect(),
                stdin.to_vec(),
            ));
            Ok(self.ran.clone())
        }
    }

    #[test]
    fn flatpak_is_detected_by_either_signal() {
        assert!(!in_flatpak(None, false));
        assert!(!in_flatpak(Some(""), false));
        assert!(in_flatpak(Some("com.krypteia.Pelican"), false));
        assert!(in_flatpak(None, true));
    }

    #[test]
    fn the_embedded_rule_is_the_shipped_rule() {
        let rule = effective(RULE);
        assert_eq!(
            rule,
            vec![
                r#"SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="091e", TAG+="uaccess""#
            ]
        );
        // The manual command single-quotes it; a quote inside would break out.
        assert!(!rule.iter().any(|l| l.contains('\'')));
    }

    #[test]
    fn install_runs_one_fixed_command_with_the_rule_on_stdin() {
        let fake = Fake::new(Some(0), "");
        let r = install_with(false, true, &fake);
        assert_eq!(r.outcome, Outcome::Installed);
        let seen = fake.seen.borrow();
        assert_eq!(seen.len(), 1);
        let (program, args, stdin) = &seen[0];
        assert_eq!(program, "/usr/bin/pkexec");
        assert_eq!(args, &["/bin/sh", "-c", SCRIPT]);
        assert_eq!(stdin.as_slice(), RULE.as_bytes());
        assert_eq!(
            SCRIPT,
            "install -m 644 /dev/stdin /etc/udev/rules.d/70-garmin-mtp.rules && udevadm control --reload \
             && udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e && udevadm settle"
        );
    }

    #[test]
    fn flatpak_and_missing_pkexec_refuse_without_running_anything() {
        for (flatpak, pkexec) in [(true, true), (false, false)] {
            let fake = Fake::new(Some(0), "");
            let r = install_with(flatpak, pkexec, &fake);
            assert_eq!(r.outcome, Outcome::Unavailable);
            assert!(
                r.message.contains("sudo install -m 644 /dev/stdin"),
                "{}",
                r.message
            );
            assert!(fake.seen.borrow().is_empty());
        }
        let s = status_with(true, true, |_| {
            panic!("a Flatpak must not read the sandbox's /etc")
        });
        assert_eq!(s.state, RuleState::Unknown);
        assert!(!s.can_install);
    }

    #[test]
    fn outcomes_are_mapped() {
        let ran = |code, stderr: &str| Ran {
            code,
            stderr: stderr.into(),
        };
        assert_eq!(outcome(&ran(Some(0), "")).outcome, Outcome::Installed);
        let c = outcome(&ran(Some(126), ""));
        assert_eq!(c.outcome, Outcome::Canceled);
        assert_eq!(
            c.message,
            "Not installed: you canceled the password prompt."
        );
        assert_eq!(
            outcome(&ran(
                Some(127),
                "Error executing command as another user: Not authorized\n"
            ))
            .outcome,
            Outcome::Canceled
        );
        assert_eq!(outcome(&ran(Some(127), "")).outcome, Outcome::Canceled);
        let agent = outcome(&ran(
            Some(127),
            "Error executing command as another user: No authentication agent found.",
        ));
        assert_eq!(agent.outcome, Outcome::Failed);
        assert!(agent.message.contains("polkit agent"));
        // sh's own 127 is a real failure, and says what went wrong.
        let sh = outcome(&ran(
            Some(127),
            "/bin/sh: line 1: udevadm: command not found\n",
        ));
        assert_eq!(sh.outcome, Outcome::Failed);
        assert!(
            sh.message.contains("udevadm: command not found"),
            "{}",
            sh.message
        );
        let f = outcome(&ran(
            Some(1),
            "install: cannot create regular file: Read-only file system\n",
        ));
        assert_eq!(f.outcome, Outcome::Failed);
        assert!(f.message.contains("Read-only file system") && f.message.contains("exit 1"));
        assert!(outcome(&ran(None, ""))
            .message
            .contains("killed by a signal"));
    }

    #[test]
    fn status_compares_the_effective_rule_and_etc_shadows_lib() {
        let etc = Path::new(ETC_RULE);
        let lib = Path::new(LIB_RULE);
        let only =
            |which: &'static Path, text: String| move |p: &Path| (p == which).then(|| text.clone());

        let s = status_with(false, true, |_| None);
        assert_eq!((s.state, s.path.as_deref()), (RuleState::Missing, None));
        assert!(s.can_install && s.why_not.is_none());

        let s = status_with(false, true, only(lib, RULE.to_string()));
        assert_eq!(
            (s.state, s.path.as_deref()),
            (RuleState::Current, Some(LIB_RULE))
        );

        // Comments and blank lines are not the rule; the manual install
        // writes only the effective line and still counts as current.
        let bare = format!("{}\n", effective(RULE).join("\n"));
        assert_eq!(
            status_with(false, true, only(etc, bare)).state,
            RuleState::Current
        );

        let old = r#"SUBSYSTEM=="usb", ATTR{idVendor}=="091e", MODE="0666""#.to_string();
        let s = status_with(false, true, only(etc, old));
        assert_eq!(
            (s.state, s.path.as_deref()),
            (RuleState::Outdated, Some(ETC_RULE))
        );

        // An outdated /etc file shadows a current packaged one.
        let s = status_with(false, true, |p: &Path| {
            Some(if p == etc {
                "# empty\n".to_string()
            } else {
                RULE.to_string()
            })
        });
        assert_eq!(s.state, RuleState::Outdated);

        assert_eq!(s.command, format!("/usr/bin/pkexec /bin/sh -c '{SCRIPT}'"));
        assert!(!status_with(false, false, |_| None).can_install);
    }

    #[test]
    fn stderr_is_summarized() {
        assert_eq!(summary("\n a \n\n b\nc\nd\n"), "b c d");
        assert!(summary(&"x".repeat(500)).ends_with('…'));
    }
}
