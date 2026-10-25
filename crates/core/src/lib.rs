//! Sluis's data model, with no GTK dependency.
//!
//! - [`strategy`]: nfqws arguments parsed into a checked model
//! - [`describe`]: names for strategies ("Fake packet, TTL 8")
//! - [`config`]: `/opt/zapret/config` read and edited without loss
//! - [`site`]: what people type or paste, as plain domain names
//! - [`services`]: popular services and all the domains they use
//! - [`options`]: nfqws's options and which a strategy may use

pub mod config;
pub mod describe;
pub mod options;
pub mod services;
pub mod site;
pub mod strategy;

pub use config::Config;
pub use describe::describe;
pub use strategy::{ParseError, Strategy};
