//! Pelican's engine — everything that knows about Garmin watches, and
//! nothing that knows about drawing.
//!
//! The split exists so a second front-end (a native macOS app) can sit on
//! the same transport, transcode pipeline and job planner as the CLI and
//! the egui GUI, rather than reimplementing them. Anything in here must
//! build and pass tests on every platform Pelican targets; anything that
//! needs a window belongs in a front-end crate.
//!
//! Front-ends talk to the engine through two things: [`transfer::run`],
//! which drains a plan and emits [`transfer::Event`]s, and [`mtp::Backend`],
//! which is the whole device API in eight methods.

pub mod garmin;
pub mod history;
pub mod mtp;
pub mod paths;
pub mod platform;
pub mod playlist;
pub mod transcode;
pub mod transfer;
