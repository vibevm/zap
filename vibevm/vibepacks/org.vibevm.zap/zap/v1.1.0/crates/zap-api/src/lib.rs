#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
);

mod commands;
mod registration;
mod surface;

pub use commands::*;
pub use registration::{capability_set, query_set};
pub use surface::*;
