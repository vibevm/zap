#![forbid(unsafe_code)]

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE");

mod capability_cache;
mod capability_goal;
mod claims;
mod coordinator;
mod driver;
mod goals;
mod indexes;
mod job_ingress;
mod liveness;
mod native_bridge;
mod protocol;
mod reconciliation;
mod records;
mod registration;
mod repair;
mod retry;
mod runtime_updates;
mod scheduler;
mod spawn_recovery;
mod state;
mod stop_ingress;
mod transitions;

pub use capability_cache::*;
pub use capability_goal::*;
pub use claims::*;
pub use coordinator::*;
pub use driver::*;
pub use goals::*;
pub use indexes::{runtime_index_algorithms, runtime_index_families};
pub use job_ingress::*;
pub use liveness::*;
pub use native_bridge::*;
pub use protocol::*;
pub use reconciliation::*;
pub use records::*;
pub use registration::{
    RuntimeAffectedJobProvider, affected_job_provider, cell_set, completion_provider_set,
    record_set, route_set,
};
pub use repair::*;
pub use retry::*;
pub use runtime_updates::*;
pub use scheduler::*;
pub use spawn_recovery::*;
pub use state::*;
pub use stop_ingress::*;
pub use transitions::*;
