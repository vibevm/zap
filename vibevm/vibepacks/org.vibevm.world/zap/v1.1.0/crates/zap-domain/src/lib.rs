#![forbid(unsafe_code)]

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root");

pub mod acceptance;
mod admission_indexes;
mod basis_indexes;
pub mod control;
pub mod dreamer;
pub mod economics;
pub mod intent;
pub mod knowledge;
pub mod legacy_projection;
pub mod lowering;
pub mod map_assessment;
pub mod owner_control;
pub mod seams;
pub mod strategic_map;
mod viewer_indexes;
pub mod viewer_queries;

pub use admission_indexes::{
    admission_index_algorithms, admission_index_families, admission_index_families_for_records,
    ensure_runtime_start_unblocked,
};
pub use basis_indexes::{
    basis_index_algorithms, basis_index_families, basis_index_families_for_records,
};
pub use viewer_indexes::{
    decode_work_ready_index_entry, viewer_graph_index_families, viewer_index_algorithms,
    viewer_index_families_for_records, work_ready_index,
};

mod queries;
mod registration;

pub use registration::{
    cell_set, completion_provider_set, control_completion_provider_set,
    economics_completion_provider_set, query_set, record_set, route_set,
};
