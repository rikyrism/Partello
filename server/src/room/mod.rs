//! Room domain: models, playback, queue, and the pure state machine.

pub mod model;
pub mod playback;
pub mod queue;
pub mod state;

#[cfg(test)]
mod state_tests;
