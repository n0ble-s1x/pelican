//! Remote names: `pl{counter:05}-{slug}.mp3`, and never one that is taken.
//!
//! MTP cannot overwrite. Sending a name that already exists leaves two
//! stubs the firmware will not let go of (libmtp #307), so the only safe
//! name is one that has never been used on this watch. A per-device counter
//! from the ledger makes that the default rather than something to detect:
//! there is no overwrite path and no "rename on collision" branch, because a
//! fresh counter is always free.
//!
//! "Taken" is still checked, belt and braces, against everything knowable:
//! every name in `/Music` as listed at the start of the run, and every name
//! in the ledger whatever its status. Stubs are counted but cannot be
//! matched: their names are unreadable, which is exactly why the ledger,
//! not the listing, is the record of what Pelican has written. The
//! counter is also pushed one step past the highest known name for every
//! stub; see [`Names::new`].

use std::collections::HashSet;

use anyhow::{anyhow, Result};

use crate::ledger::Ledger;
use crate::mtp::{fold_name, RemoteEntry};
use crate::transcode::sanitize_filename_stem;

pub const PREFIX: &str = "pl";
pub const EXT: &str = ".mp3";
/// FR165 firmware drops writes whose filename runs past about 60 chars;
/// 56 leaves room for `.mp3`.
pub const MAX_STEM: usize = 56;
/// Device names whose counter is above this do not move the run's counter.
/// Pelican gets nowhere near it (a watch holds 500 tracks), so such a name
/// was put there by something else, and following it would walk the
/// counter to `u64::MAX` and off the end. The name itself is still taken.
pub const MAX_DEVICE_COUNTER: u64 = 1_000_000_000;

/// The remote name for `counter`, slugged from `title`.
///
/// The slug is the existing [`sanitize_filename_stem`] of the title, cut
/// again so the whole stem, prefix included, stays within [`MAX_STEM`].
/// The prefix is never cut: it is what makes the name unique.
pub fn remote_name(counter: u64, title: &str) -> String {
    let prefix = format!("{PREFIX}{counter:05}-");
    let budget = MAX_STEM.saturating_sub(prefix.len());
    let slug: String = sanitize_filename_stem(title).chars().take(budget).collect();
    let slug = slug.trim_end_matches(['-', ' ', '.']);
    if slug.is_empty() {
        format!("{}{EXT}", prefix.trim_end_matches('-'))
    } else {
        format!("{prefix}{slug}{EXT}")
    }
}

/// The counter in a Pelican-shaped name, any case: `pl00012-x.mp3` and
/// `PL0001.mp3` (the hand-sent hardware proof) both count.
pub fn parse_counter(name: &str) -> Option<u64> {
    let folded = fold_name(name);
    let rest = folded.strip_prefix(PREFIX)?;
    let digits: &str = &rest[..rest.find(|c: char| !c.is_ascii_digit())?];
    let after = &rest[digits.len()..];
    if digits.is_empty() || !(after.starts_with('-') || after.starts_with('.')) {
        return None;
    }
    digits.parse().ok()
}

/// Every name this run may not use, and the counter it hands out next.
#[derive(Debug, Clone)]
pub struct Names {
    taken: HashSet<String>,
    next: u64,
    stubs: usize,
    unaccounted: usize,
}

impl Names {
    /// Built once per run, from the one listing of `/Music` and the ledger.
    ///
    /// The first counter is `max(ledger counters, pl-NNNNN names on the
    /// device) + 1`: a name the ledger lost (another machine, a restored
    /// home dir) but the watch still lists pushes the counter past it.
    ///
    /// Stubs hide their names, so they cannot push the counter that way.
    /// A stub this ledger never saw is most likely one of the last writes
    /// some other ledger made, just above the highest readable name, so
    /// the counter skips one step per stub, every stub. The ledger's own
    /// failed writes are not credited against them: a failed write may
    /// have left no object at all (it died before `SendObjectInfo`) or a
    /// readable one, and a failure wrongly taken as "the stub" would put
    /// the counter back on the stub's hidden name. Skipped counters cost
    /// nothing; a reused name costs two more stubs (libmtp #307).
    ///
    /// That covers the common case (a lost ledger whose final uploads were
    /// cut off); it cannot cover a stub whose counter sits past a gap,
    /// which is why losing the ledger is still reported to the user rather
    /// than trusted away.
    pub fn new(listing: &[RemoteEntry], ledger: &Ledger) -> Result<Self> {
        let mut taken: HashSet<String> = ledger.names().collect();
        let mut max = ledger.max_counter();
        let mut stubs = 0usize;
        let mut readable = HashSet::new();
        for e in listing {
            if e.is_broken {
                stubs += 1;
                continue;
            }
            readable.insert(fold_name(&e.name));
            if let Some(c) = parse_counter(&e.name).filter(|c| *c <= MAX_DEVICE_COUNTER) {
                max = max.max(c);
            }
        }
        // Only for the warning: which stubs this ledger may have made. A
        // failed or unresolved name the listing shows readable is not one.
        let could_be_stubs = ledger
            .unproven_names()
            .filter(|n| !readable.contains(n))
            .count();
        let unaccounted = stubs.saturating_sub(could_be_stubs);
        taken.extend(readable);
        let next = u64::try_from(stubs)
            .ok()
            .and_then(|u| max.checked_add(1)?.checked_add(u))
            .ok_or_else(|| counter_exhausted(ledger))?;
        Ok(Self {
            taken,
            next,
            stubs,
            unaccounted,
        })
    }

    pub fn is_taken(&self, name: &str) -> bool {
        self.taken.contains(&fold_name(name))
    }

    /// Unreadable objects in the listing. Their names are unknowable; see
    /// the module docs.
    pub fn stubs(&self) -> usize {
        self.stubs
    }

    /// Stubs beyond what this ledger's failed and unresolved writes could
    /// have left: writes some other ledger made. An estimate for telling
    /// the user; the counter does not depend on it.
    pub fn unaccounted_stubs(&self) -> usize {
        self.unaccounted
    }

    /// The counter the next [`Names::allocate`] will try first.
    pub fn next_counter(&self) -> u64 {
        self.next
    }

    /// A fresh `(counter, name)`, marked taken before it is returned.
    ///
    /// The counter only moves forward. The loop never turns in practice
    /// (every taken `pl` name is below the starting counter), but if some
    /// foreign file ever did hold a future name, this skips it rather than
    /// writing over it. Running out of counters is an error, never a wrap
    /// back to 0.
    pub fn allocate(&mut self, title: &str) -> Result<(u64, String)> {
        loop {
            let counter = self.next;
            self.next = counter
                .checked_add(1)
                .ok_or_else(|| anyhow!("the name counter is exhausted at {counter}"))?;
            let name = remote_name(counter, title);
            if self.taken.insert(fold_name(&name)) {
                return Ok((counter, name));
            }
        }
    }
}

fn counter_exhausted(ledger: &Ledger) -> anyhow::Error {
    anyhow!(
        "the ledger {} holds a counter too large to continue from ({}). \
         Pelican will not wrap the counter and risk reusing a name.",
        ledger.path().display(),
        ledger.max_counter()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Event, Kind};

    fn entry(name: &str) -> RemoteEntry {
        RemoteEntry {
            name: name.into(),
            path: format!("Music/{name}"),
            size: 1,
            is_folder: false,
            is_broken: false,
            handle: 0,
        }
    }

    fn stub(handle: u32) -> RemoteEntry {
        RemoteEntry {
            name: format!("‹unreadable #{handle}›"),
            is_broken: true,
            handle,
            ..entry("")
        }
    }

    #[test]
    fn names_have_the_counter_and_the_slug() {
        assert_eq!(remote_name(1, "Maiden Voyage"), "pl00001-Maiden Voyage.mp3");
        assert_eq!(remote_name(123_456, "x"), "pl123456-x.mp3");
        assert_eq!(
            remote_name(7, "Track #3: <Live!>"),
            "pl00007-Track -3- -Live.mp3"
        );
    }

    #[test]
    fn the_stem_never_passes_56_and_never_ends_in_punctuation() {
        for title in [
            "a".repeat(200),
            format!("{}-{}", "b".repeat(47), "c".repeat(20)),
            "Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time".into(),
        ] {
            for counter in [1, 99_999, 1_000_000] {
                let name = remote_name(counter, &title);
                let stem = name.strip_suffix(".mp3").unwrap();
                assert!(stem.chars().count() <= MAX_STEM, "{name}");
                assert!(!stem.ends_with(['-', ' ', '.']), "{name}");
                assert_eq!(parse_counter(&name), Some(counter), "{name}");
            }
        }
    }

    #[test]
    fn counters_parse_from_our_names_only() {
        assert_eq!(parse_counter("pl00012-x.mp3"), Some(12));
        assert_eq!(parse_counter("PL0001.mp3"), Some(1));
        assert_eq!(parse_counter("playlist.mp3"), None);
        assert_eq!(parse_counter("pl.mp3"), None);
        assert_eq!(parse_counter("pl12x.mp3"), None);
        assert_eq!(parse_counter("Track.mp3"), None);
    }

    fn ledger_with(dir: &std::path::Path, names: &[(u64, &str)]) -> Ledger {
        let mut l = Ledger::open(dir, "1").unwrap();
        for (i, (c, n)) in names.iter().enumerate() {
            // Status does not matter: reserve, verified and failed names
            // are all burned.
            let kind = [Kind::Reserve, Kind::Failed, Kind::Verified][i % 3];
            l.append(Event::new(kind, *c, n)).unwrap();
        }
        l
    }

    #[test]
    fn counter_starts_above_ledger_and_device() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(tmp.path(), &[(4, "pl00004-a.mp3")]);
        let mut n = Names::new(&[entry("PL00009-b.MP3"), entry("song.mp3")], &l).unwrap();
        assert_eq!(n.next_counter(), 10);
        assert_eq!(n.allocate("c").unwrap(), (10, "pl00010-c.mp3".into()));
        assert_eq!(n.allocate("c").unwrap(), (11, "pl00011-c.mp3".into()));

        let l = ledger_with(tempfile::tempdir().unwrap().path(), &[(40, "x")]);
        let n = Names::new(&[entry("pl00009-b.mp3")], &l).unwrap();
        assert_eq!(n.next_counter(), 41);
    }

    #[test]
    fn device_ledger_and_allocated_names_are_all_taken_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(
            tmp.path(),
            &[(1, "pl00001-a.mp3"), (2, "pl00002-A.mp3"), (3, "Old.mp3")],
        );
        let mut n = Names::new(&[entry("Foreign Song.MP3"), stub(77), stub(78)], &l).unwrap();
        for name in [
            "PL00001-A.MP3",
            "pl00002-a.mp3",
            "old.MP3",
            "foreign song.mp3",
        ] {
            assert!(n.is_taken(name), "{name}");
        }
        assert_eq!(n.stubs(), 2);
        // Stub names are unknowable and their synthetic labels are not
        // names on the device, so they are not entered as taken strings.
        assert!(!n.is_taken("‹unreadable #77›"));
        let (_, fresh) = n.allocate("a").unwrap();
        assert!(n.is_taken(&fresh.to_uppercase()));
    }

    #[test]
    fn a_future_name_already_taken_is_skipped_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(tmp.path(), &[(1, "pl00001-a.mp3")]);
        let mut n = Names::new(&[], &l).unwrap();
        // Some other tool planted the exact name counter 2 would produce.
        // A listing would have moved the counter past it; forcing it into
        // the taken set directly shows the allocator's own guard.
        n.taken.insert(fold_name("PL00002-SONG.mp3"));
        assert_eq!(n.allocate("song").unwrap(), (3, "pl00003-song.mp3".into()));
    }

    /// A foreign file with an absurd counter must not push the next counter
    /// past `u64::MAX` (a panic in debug, a wrap to 0 in release).
    #[test]
    fn an_absurd_device_counter_is_taken_but_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(tmp.path(), &[(7, "pl00007-a.mp3")]);
        let huge = format!("pl{}-x.mp3", u64::MAX);
        assert_eq!(parse_counter(&huge), Some(u64::MAX));
        let over = format!("pl{}-y.mp3", MAX_DEVICE_COUNTER + 1);
        let at = format!("pl{MAX_DEVICE_COUNTER}-z.mp3");

        let mut n = Names::new(&[entry(&huge), entry(&over)], &l).unwrap();
        assert!(n.is_taken(&huge) && n.is_taken(&over));
        assert_eq!(n.allocate("b").unwrap(), (8, "pl00008-b.mp3".into()));

        // At the bound it is still followed.
        let n = Names::new(&[entry(&at)], &l).unwrap();
        assert_eq!(n.next_counter(), MAX_DEVICE_COUNTER + 1);
    }

    #[test]
    fn an_exhausted_counter_is_an_error_not_a_wrap() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(tmp.path(), &[(u64::MAX, "pl-max.mp3")]);
        let err = Names::new(&[], &l).unwrap_err().to_string();
        assert!(err.contains("will not wrap"), "{err}");

        let l = ledger_with(tempfile::tempdir().unwrap().path(), &[(1, "a")]);
        let mut n = Names::new(&[], &l).unwrap();
        n.next = u64::MAX - 1;
        assert_eq!(n.allocate("z").unwrap().0, u64::MAX - 1);
        // u64::MAX itself is never handed out: there is no counter after it.
        assert!(n.allocate("z").is_err());
        assert!(n.allocate("z").is_err(), "and it stays exhausted");
    }

    /// The reviewer's case: the last upload of an old run, counter 26, was
    /// cut off and left a stub, and this machine has no ledger for the
    /// watch. The readable names stop at 25; starting at 26 would rebuild
    /// the stub's hidden name.
    #[test]
    fn stubs_the_ledger_cannot_explain_push_the_counter() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = Ledger::open(tmp.path(), "1").unwrap();
        let mut listing: Vec<_> = (1..=25).map(|c| entry(&remote_name(c, "Track"))).collect();
        listing.push(stub(900));
        let n = Names::new(&listing, &empty).unwrap();
        assert_eq!(n.unaccounted_stubs(), 1);
        assert_eq!(n.next_counter(), 27);

        // A ledger that recorded the cut-off write explains the stub for
        // the warning; the counter still steps past it, which costs one
        // unused number.
        let l = ledger_with(
            tempfile::tempdir().unwrap().path(),
            &[(26, "pl00026-Maiden Voyage.mp3")],
        );
        let n = Names::new(&listing, &l).unwrap();
        assert_eq!(n.unaccounted_stubs(), 0);
        assert_eq!(n.next_counter(), 28);
    }

    /// Round 2's case: the stub is another machine's cut-off write of
    /// counter 26, and this ledger's only failure left no object at all.
    /// Crediting that failure against the stub put the counter on 26 and
    /// rebuilt the stub's hidden name.
    #[test]
    fn a_failure_that_left_no_stub_does_not_explain_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        let mut failed = Event::new(Kind::Failed, 3, "pl00003-Other.mp3");
        failed.reason = Some("upload failed: opening /tmp/staged/0.mp3".into());
        l.append(Event::new(Kind::Reserve, 3, "pl00003-Other.mp3"))
            .unwrap();
        l.append(failed).unwrap();
        let mut listing: Vec<_> = (1..=25).map(|c| entry(&remote_name(c, "Track"))).collect();
        listing.push(stub(900));
        let mut n = Names::new(&listing, &l).unwrap();
        assert_eq!(n.next_counter(), 27);
        assert_ne!(
            n.allocate("Maiden Voyage").unwrap().1,
            "pl00026-Maiden Voyage.mp3"
        );
    }

    /// A failed write whose object the listing shows readable (a read-back
    /// mismatch) is not a stub, so it does not hide a foreign stub from
    /// the warning either.
    #[test]
    fn a_readable_failed_name_is_not_counted_as_a_stub() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        l.append(Event::new(Kind::Reserve, 25, "pl00025-Track.mp3"))
            .unwrap();
        l.append(Event::new(Kind::Failed, 25, "pl00025-Track.mp3"))
            .unwrap();
        // Unresolved and not in the listing: this one may be the stub.
        l.append(Event::new(Kind::Reserve, 24, "pl00024-Gone.mp3"))
            .unwrap();
        let mut listing: Vec<_> = (1..=25).map(|c| entry(&remote_name(c, "Track"))).collect();
        listing.push(stub(900));
        listing.push(stub(901));
        let n = Names::new(&listing, &l).unwrap();
        assert_eq!(n.stubs(), 2);
        assert_eq!(n.unaccounted_stubs(), 1);
        assert_eq!(n.next_counter(), 28);
    }
}
