//! Remote AI for the map: what the window asks the model, and what it does with the answer.
//!
//! The model is remote by design: the suite is pure Rust and llama.cpp is C/C++, so the local GGUF
//! engine the Electron app had does not come along (it is a declared divergence). What a user
//! configures is a connection: a provider preset, its base URL, a model and their own key, which
//! the OS keychain holds through the shared `local-ai` crate.
//!
//! Three pieces, each testable without a network:
//!
//! - [`prompts`]: the fragments, localized, with the JSON contract written out in the prompt (the
//!   contract's keys are never translated).
//! - [`extract`]: the tolerant parser for what a model actually answers - reasoning blocks, code
//!   fences, prose around the JSON, trailing commas.
//! - [`actions`]: the five actions and the chat, each turning a request plus the map into messages
//!   and the answer back into nodes.
//!
//! [`backend::Backend`] is the seam to the network: the app uses [`backend::LiveBackend`] (the
//! shared `local-ai` client), and the tests use a mock that answers from a canned string.

pub mod actions;
pub mod backend;
pub mod extract;
pub mod outline;
pub mod prompts;

pub use actions::{Outcome, Request};
pub use backend::{Backend, LiveBackend};
pub use outline::{MAP_BUDGET, Outline, map_outline};
