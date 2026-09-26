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
//! matched — their names are unreadable, which is exactly why the ledger,
//! not the listing, is the record of what Pelican has written.

use std::collections::HashSet;

use crate::ledger::Ledger;
use crate::mtp::{fold_name, RemoteEntry};
use crate::transcode::sanitize_filename_stem;

pub const PREFIX: &str = "pl";
pub const EXT: &str = ".mp3";
/// FR165 firmware drops writes whose filename runs past about 60 chars;
/// 56 leaves room for `.mp3`.
pub const MAX_STEM: usize = 56;

/// The remote name for `counter`, slugged from `title`.
///
/// The slug is the existing [`sanitize_filename_stem`] of the title, cut
/// again so the whole stem — prefix included — stays within [`MAX_STEM`].
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
}

impl Names {
    /// Built once per run, from the one listing of `/Music` and the ledger.
    ///
    /// The first counter is `max(ledger counters, pl-NNNNN names on the
    /// device) + 1`: a name the ledger lost (another machine, a restored
    /// home dir) but the watch still lists pushes the counter past it.
    pub fn new(listing: &[RemoteEntry], ledger: &Ledger) -> Self {
        let mut taken: HashSet<String> = ledger.names().collect();
        let mut max = ledger.max_counter();
        let mut stubs = 0;
        for e in listing {
            if e.is_broken {
                stubs += 1;
                continue;
            }
            taken.insert(fold_name(&e.name));
            if let Some(c) = parse_counter(&e.name) {
                max = max.max(c);
            }
        }
        Self {
            taken,
            next: max + 1,
            stubs,
        }
    }

    pub fn is_taken(&self, name: &str) -> bool {
        self.taken.contains(&fold_name(name))
    }

    /// Unreadable objects in the listing. Their names are unknowable; see
    /// the module docs.
    pub fn stubs(&self) -> usize {
        self.stubs
    }

    /// The counter the next [`Names::allocate`] will try first.
    pub fn next_counter(&self) -> u64 {
        self.next
    }

    /// A fresh `(counter, name)`, marked taken before it is returned.
    ///
    /// The counter only moves forward. The loop never turns in practice —
    /// every taken `pl` name is below the starting counter — but if some
    /// foreign file ever did hold a future name, this skips it rather than
    /// writing over it.
    pub fn allocate(&mut self, title: &str) -> (u64, String) {
        loop {
            let counter = self.next;
            self.next += 1;
            let name = remote_name(counter, title);
            if self.taken.insert(fold_name(&name)) {
                return (counter, name);
            }
        }
    }
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
        let mut n = Names::new(&[entry("PL00009-b.MP3"), entry("song.mp3")], &l);
        assert_eq!(n.next_counter(), 10);
        assert_eq!(n.allocate("c"), (10, "pl00010-c.mp3".into()));
        assert_eq!(n.allocate("c"), (11, "pl00011-c.mp3".into()));

        let l = ledger_with(tempfile::tempdir().unwrap().path(), &[(40, "x")]);
        assert_eq!(Names::new(&[entry("pl00009-b.mp3")], &l).next_counter(), 41);
    }

    #[test]
    fn device_ledger_and_allocated_names_are_all_taken_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(
            tmp.path(),
            &[(1, "pl00001-a.mp3"), (2, "pl00002-A.mp3"), (3, "Old.mp3")],
        );
        let mut n = Names::new(&[entry("Foreign Song.MP3"), stub(77), stub(78)], &l);
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
        let (_, fresh) = n.allocate("a");
        assert!(n.is_taken(&fresh.to_uppercase()));
    }

    #[test]
    fn a_future_name_already_taken_is_skipped_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let l = ledger_with(tmp.path(), &[(1, "pl00001-a.mp3")]);
        let mut n = Names::new(&[], &l);
        // Some other tool planted the exact name counter 2 would produce.
        // A listing would have moved the counter past it; forcing it into
        // the taken set directly shows the allocator's own guard.
        n.taken.insert(fold_name("PL00002-SONG.mp3"));
        assert_eq!(n.allocate("song"), (3, "pl00003-song.mp3".into()));
    }
}
