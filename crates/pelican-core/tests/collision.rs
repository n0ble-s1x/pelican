//! The plan-time half of the collision guard.
//!
//! Every case here is a way to lose a file the user already had. Per
//! `docs/garmin-mtp.md` §7, a same-name write on an FR165 running FW 2506
//! turns *both* objects into unreadable stubs, and per §6 no `DeleteObject`
//! against a stub has ever succeeded — so a miss is unrecoverable, and the
//! comparison is deliberately biased toward over-reporting.
//!
//! No device is needed: `resolve_conflicts` is pure, and takes the listing
//! its caller already had to make.

use std::path::PathBuf;

use pelican_core::transfer::{resolve_conflicts, Job, OnConflict};

fn job(src: &str, remote_name: &str) -> Job {
    Job {
        src: PathBuf::from(src),
        remote_dir: "Music".into(),
        remote_name: remote_name.into(),
        name_checked: false,
    }
}

fn on_watch(names: &[&str]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|n| ("Music".to_string(), n.to_string()))
        .collect()
}

fn names(jobs: &[Job]) -> Vec<&str> {
    jobs.iter().map(|j| j.remote_name.as_str()).collect()
}

/// The base case, and the one the whole guard exists for: send `track.mp3` on
/// Monday, send a *different* `track.mp3` on Tuesday. Nothing in the planner
/// used to compare the two.
#[test]
fn a_name_already_on_the_watch_is_not_planned_over() {
    let mut jobs = vec![job("/tuesday/track.mp3", "track.mp3")];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["track.mp3"]), OnConflict::Skip, true);

    assert!(
        jobs.is_empty(),
        "the job must not survive: {:?}",
        names(&jobs)
    );
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].existing, "track.mp3");
    assert_eq!(conflicts[0].wanted, "track");
    assert_eq!(conflicts[0].renamed_to, None);
}

/// A file the plan was never going to upload cannot collide with anything.
/// Reporting `cover.jpg` as "already on your watch" would be a false alarm
/// about a file that was always going to be skipped.
#[test]
fn a_file_that_is_never_uploaded_is_not_a_conflict() {
    let mut jobs = vec![job("/a/cover.jpg", "cover.jpg")];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["cover.mp3"]), OnConflict::Skip, true);
    assert!(conflicts.is_empty());
    assert_eq!(jobs.len(), 1, "it still gets its own skip at upload time");
}

/// `sanitize_filename_stem` truncates to 56 chars, so two names that differ
/// only past character 56 land on the watch as one name. One of them is
/// already there.
#[test]
fn collision_is_detected_after_truncation() {
    let shared = "x".repeat(56);
    let already_written = format!("{shared}.mp3");
    // 80 chars, distinct from whatever produced the file on the watch, and
    // identical to it for the first 56.
    let incoming = format!("{shared}{}.mp3", "B".repeat(24));

    let mut jobs = vec![job(&format!("/b/{incoming}"), &incoming)];
    let conflicts = resolve_conflicts(
        &mut jobs,
        &on_watch(&[&already_written]),
        OnConflict::Skip,
        true,
    );

    assert!(jobs.is_empty(), "truncation collision missed");
    assert_eq!(conflicts[0].existing, already_written);
}

/// `/Music` is FAT-derived. `Track.mp3` and `track.mp3` are one file to the
/// firmware and two strings to us.
#[test]
fn collision_is_detected_case_insensitively() {
    let mut jobs = vec![job("/a/track.mp3", "track.mp3")];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["Track.mp3"]), OnConflict::Skip, true);

    assert!(jobs.is_empty());
    assert_eq!(
        conflicts[0].existing, "Track.mp3",
        "the message must use the watch's own spelling"
    );
}

/// The final extension is not known until the encoder plans, so the
/// comparison is on the stem. That over-reports across extensions and never
/// under-reports, which is the correct direction when a miss is
/// unrecoverable.
#[test]
fn stem_match_counts_even_when_the_extension_differs() {
    let mut jobs = vec![job("/a/song.flac", "song.flac")];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["song.mp3"]), OnConflict::Skip, true);
    assert!(jobs.is_empty());
    assert_eq!(conflicts[0].existing, "song.mp3");
}

/// Rename is the user's explicit answer to a reported collision, so it has to
/// land somewhere genuinely free — on the watch *and* in the plan — and stay
/// inside Garmin's 56-char stem budget.
#[test]
fn rename_targets_a_name_free_on_both_sides_and_within_the_cap() {
    let mut jobs = vec![job("/a/song.mp3", "song.mp3")];
    let conflicts = resolve_conflicts(
        &mut jobs,
        &on_watch(&["song.mp3", "song-2.mp3"]),
        OnConflict::Rename,
        true,
    );

    assert_eq!(names(&jobs), vec!["song-3.mp3"]);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].renamed_to.as_deref(), Some("song-3.mp3"));
    assert_eq!(
        conflicts[0].existing, "song.mp3",
        "the name it could not have is the one worth reporting"
    );
}

#[test]
fn a_rename_stays_inside_the_fifty_six_char_stem_budget() {
    let long = "y".repeat(56);
    let mut jobs = vec![job(&format!("/a/{long}.mp3"), &format!("{long}.mp3"))];
    resolve_conflicts(
        &mut jobs,
        &on_watch(&[&format!("{long}.mp3")]),
        OnConflict::Rename,
        true,
    );

    let stem_len = jobs[0].remote_name.rsplit_once('.').unwrap().0.len();
    assert!(
        stem_len <= 56,
        "{} is {stem_len} chars",
        jobs[0].remote_name
    );
    assert_ne!(jobs[0].remote_name, format!("{long}.mp3"));
}

/// The device side is never sanitized. A Garmin-Express-written `Café.mp3`
/// sanitizes to `Caf`, which would falsely block a planned `Caf.mp3` that in
/// fact targets a free name — and the user would be told a file was on their
/// watch that is not.
#[test]
fn a_device_name_is_matched_raw_not_sanitised() {
    let mut jobs = vec![job("/a/Caf.mp3", "Caf.mp3")];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["Café.mp3"]), OnConflict::Skip, true);

    assert!(conflicts.is_empty(), "false collision: {conflicts:?}");
    assert_eq!(names(&jobs), vec!["Caf.mp3"]);
}

/// A collision in `Music` says nothing about a name in `Music/Runs`.
#[test]
fn conflicts_are_scoped_per_remote_dir() {
    let mut other = Job {
        remote_dir: "Music/Runs".into(),
        ..job("/a/track.mp3", "track.mp3")
    };
    other.name_checked = false;
    let mut jobs = vec![other];
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["track.mp3"]), OnConflict::Skip, true);
    assert!(conflicts.is_empty());
    assert_eq!(jobs.len(), 1);
}

/// Everything that survives carries the evidence, because that is what the
/// write-time guard consults when its own listing fails.
#[test]
fn a_surviving_job_is_marked_as_checked_against_a_listing() {
    let mut jobs = vec![job("/a/one.mp3", "one.mp3"), job("/a/two.mp3", "two.mp3")];
    resolve_conflicts(&mut jobs, &on_watch(&["other.mp3"]), OnConflict::Skip, true);
    assert!(jobs.iter().all(|j| j.name_checked));
}

/// Two different albums, one basename, and the watch already holds it. Both
/// must be refused — not one refused and one renamed into the other's slot.
#[test]
fn every_plan_job_colliding_with_one_device_name_is_refused() {
    let mut jobs = vec![
        job("/a/track.mp3", "track.mp3"),
        job("/b/track.mp3", "track.mp3"),
    ];
    // The plan-only dedupe would already have moved the second to `track-2`;
    // this is the harder shape, where it has not.
    let conflicts = resolve_conflicts(&mut jobs, &on_watch(&["track.mp3"]), OnConflict::Skip, true);
    assert!(jobs.is_empty());
    assert_eq!(conflicts.len(), 2);
}
