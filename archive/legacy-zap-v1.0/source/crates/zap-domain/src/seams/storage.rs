macro_rules! impl_stored_record {
    ($record:ty, $key:ty, $key_field:ident, $version_field:ident, $family:literal, $indexes:path) => {
        impl zap_core::StoredRecord for $record {
            type Key = $key;
            type Version = zap_wire::Revision;
            const FAMILY: &'static str = $family;

            fn key(&self) -> Self::Key {
                self.$key_field.clone()
            }
            fn version(&self) -> Self::Version {
                self.$version_field
            }
            fn descriptor() -> Result<zap_core::RecordDescriptor, zap_wire::ZapError> {
                Ok(zap_core::RecordDescriptor {
                    family: zap_core::RecordFamily::parse(Self::FAMILY)?,
                    key_codec: zap_wire::CodecEpoch::CURRENT,
                    value_codec: zap_wire::CodecEpoch::CURRENT,
                    version_codec: zap_wire::CodecEpoch::CURRENT,
                })
            }
            fn index_rows(&self) -> Result<Vec<zap_core::RecordIndexRow>, zap_wire::ZapError> {
                $indexes(self)
            }
        }
    };
    ($record:ty, $key:ty, $key_field:ident, $version_field:ident, $family:literal) => {
        impl zap_core::StoredRecord for $record {
            type Key = $key;
            type Version = zap_wire::Revision;
            const FAMILY: &'static str = $family;

            fn key(&self) -> Self::Key {
                self.$key_field.clone()
            }

            fn version(&self) -> Self::Version {
                self.$version_field
            }

            fn descriptor() -> Result<zap_core::RecordDescriptor, zap_wire::ZapError> {
                Ok(zap_core::RecordDescriptor {
                    family: zap_core::RecordFamily::parse(Self::FAMILY)?,
                    key_codec: zap_wire::CodecEpoch::CURRENT,
                    value_codec: zap_wire::CodecEpoch::CURRENT,
                    version_codec: zap_wire::CodecEpoch::CURRENT,
                })
            }
        }
    };
}

pub(crate) use impl_stored_record;

pub(crate) fn scan_all<R: zap_core::StoredRecord>(
    state: &dyn zap_core::StateReader,
) -> Result<Vec<R>, zap_wire::ZapError> {
    use std::ops::Bound;
    use zap_core::{KeyRange, PageLimit, RecordCompleteness, StateReaderExt, StoredRecord};

    let limit = PageLimit::within(512, 512)?;
    let mut start = Bound::Unbounded;
    let mut records = Vec::new();
    loop {
        let page = state.scan_typed::<R>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            limit,
        )?;
        match page.completeness {
            RecordCompleteness::Complete => {
                records.extend(page.items);
                return Ok(records);
            }
            RecordCompleteness::More => {
                let Some(last) = page.items.last().map(StoredRecord::key) else {
                    return Err(zap_wire::ZapError::from_static(
                        zap_wire::ErrorCode::InternalInvariant,
                        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
                        "record scan returned a continuation without an item boundary",
                        zap_wire::FixSurface::Store,
                        zap_wire::ErrorDetail::None,
                    ));
                };
                start = Bound::Excluded(last);
                records.extend(page.items);
            }
            RecordCompleteness::UnknownBoundary => {
                return Err(zap_wire::ZapError::from_static(
                    zap_wire::ErrorCode::Unavailable,
                    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-COMPLETION-BLOCKERS",
                    "completion cannot treat an unknown record boundary as empty",
                    zap_wire::FixSurface::Store,
                    zap_wire::ErrorDetail::None,
                ));
            }
        }
    }
}
