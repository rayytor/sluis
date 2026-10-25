//! The quick scan engine, with no GTK dependency: diagnosis, candidate list,
//! parallel isolated testing and ranking (docs/PLAN.md §3.2). The helper runs it
//! as root and supplies the [`Sandbox`]: private nfqws instances, each fed
//! by its own range of source ports. [`blockcheck`] reads zapret's own
//! exhaustive scan for the Thorough Scan.

pub mod blockcheck;
pub mod candidates;
pub mod engine;
pub mod probe;

pub use engine::{Diagnosis, Event, Report, Request, Sandbox, Tried, run};
pub use probe::{Curl, Outcome, Ports, Prober, Target};
