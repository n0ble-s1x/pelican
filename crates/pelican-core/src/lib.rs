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
//! A front-end talks to the device only through [`mtp::Backend`], and to the
//! filesystem through [`source`] (what to send), [`transcode`] (what it
//! becomes) and [`staging`] (where it waits).

pub mod garmin;
pub mod mtp;
pub mod paths;
pub mod platform;
pub mod source;
pub mod staging;
pub mod transcode;
