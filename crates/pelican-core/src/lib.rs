//! Pelican's engine — everything that knows about Garmin watches, and
//! nothing that knows about drawing.
//!
//! The promise, proven on hardware (`docs/garmin-library-persistence.md`
//! § Results): transcode to one known-good profile, push under a name that
//! has never been used, and prove each file landed intact. There is no
//! delete anywhere in here — once a track is on the watch it stays in the
//! watch's library until a factory reset, so the only safe write is a new
//! one.
//!
//! A front-end drives a run through [`transfer`], which talks to the device
//! only through [`mtp::Backend`] and to the filesystem through [`source`]
//! (what to send), [`transcode`] (what it becomes) and [`staging`] (where it
//! waits). [`ledger`] is the per-device record of every name ever written,
//! and [`naming`] turns it into names that have never been used. [`watch`]
//! is the read-only side: what is on the device now, and whose it is.
//! [`preview`] shapes a plan for a front-end to show before a run, and
//! [`library`] browses the music on disk one folder at a time.

pub mod backup;
pub mod error;
pub mod garmin;
pub mod hash;
pub mod ledger;
pub mod library;
pub mod mtp;
pub mod naming;
pub mod paths;
pub mod places;
pub mod platform;
pub mod preview;
pub mod reset;
pub mod source;
pub mod staging;
pub mod transcode;
pub mod transfer;
pub mod watch;
