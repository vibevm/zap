#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

mod codec;
mod import;
mod journal;
mod mapping;
mod records;
mod registration;
mod replay;
mod snapshot;
mod source;
mod types;

pub use codec::{LegacyCodecError, LegacyTag, LegacyValue, packed, unpack};
pub use import::{LegacyImportCell, LegacyImportOutput, LegacyImportPayload};
pub use journal::{LegacyEventRecord, LegacyReadError, LegacyStoreRead, Zap1Reader};
pub use mapping::{ImportMapBuilder, SubjectTarget, deterministic_spelling};
pub use records::{
    LegacyAuthorityClass, LegacyImportManifestRecord, LegacyObjectKey, LegacyObjectRecord,
};
pub use registration::{capability_set, cell_set, query_set, record_set, route_set};
pub use replay::LegacyProjection;
pub use snapshot::LegacySnapshot;
pub use source::{LegacyInventory, LegacySource};
pub use types::{
    CurrentImportId, ImportIdMap, LegacyDiagnostic, LegacyDigest, LegacyDigestDomain, LegacyId,
    LegacyKind, PendingTail,
};
pub use zap_wire::LegacyEpoch;
