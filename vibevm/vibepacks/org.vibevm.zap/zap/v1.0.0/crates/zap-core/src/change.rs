use std::any::TypeId;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, RequirementRef, ZapError};

use crate::record::erase_record;
use crate::{
    EncodedRecordKey, ErasedRecord, RecordDescriptor, RecordFamily, RecordSet, StoredRecord,
    VersionStamp,
};

mod index_overlay;
mod record_overlay;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MutationKey {
    family: RecordFamily,
    key: EncodedRecordKey,
}

#[allow(dead_code)]
pub(crate) enum RecordMutation {
    Insert {
        descriptor: RecordDescriptor,
        type_id: TypeId,
        key: EncodedRecordKey,
        value: Arc<dyn ErasedRecord>,
    },
    Replace {
        descriptor: RecordDescriptor,
        type_id: TypeId,
        key: EncodedRecordKey,
        expected: Vec<u8>,
        value: Arc<dyn ErasedRecord>,
    },
    Remove {
        descriptor: RecordDescriptor,
        type_id: TypeId,
        key: EncodedRecordKey,
        expected: Vec<u8>,
    },
}

/// A noncommitting buffer of typed record mutations and invariant assertions.
#[derive(Default)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub struct ChangeSet {
    mutations: BTreeMap<MutationKey, RecordMutation>,
    assertions: Vec<RequirementRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub enum MutationKind {
    Insert,
    Replace,
    Remove,
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub struct PreparedRecordMutation {
    kind: MutationKind,
    descriptor: RecordDescriptor,
    key: EncodedRecordKey,
    expected_version: Option<Vec<u8>>,
    new_version: Option<Vec<u8>>,
    value: Option<Vec<u8>>,
}

impl PreparedRecordMutation {
    pub const fn kind(&self) -> MutationKind {
        self.kind
    }

    pub fn descriptor(&self) -> &RecordDescriptor {
        &self.descriptor
    }

    pub fn key(&self) -> &EncodedRecordKey {
        &self.key
    }

    pub fn expected_version(&self) -> Option<&[u8]> {
        self.expected_version.as_deref()
    }

    pub fn new_version(&self) -> Option<&[u8]> {
        self.new_version.as_deref()
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }
}

impl ChangeSet {
    /// Creates an empty, noncommitting mutation buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Retains a typed insertion for later validation by CommitService.
    pub fn insert<R: StoredRecord>(&mut self, value: R) -> Result<(), ZapError> {
        let descriptor = R::descriptor()?;
        let key = EncodedRecordKey::from_key(&value.key())?;
        let mutation_key = MutationKey {
            family: descriptor.family.clone(),
            key: key.clone(),
        };
        let value = erase_record(descriptor.clone(), value);
        self.add(
            mutation_key,
            RecordMutation::Insert {
                descriptor,
                type_id: TypeId::of::<R>(),
                key,
                value,
            },
        )
    }

    /// Retains a typed compare-and-replace for later atomic validation.
    pub fn replace<R: StoredRecord>(
        &mut self,
        expected: R::Version,
        value: R,
    ) -> Result<(), ZapError> {
        let descriptor = R::descriptor()?;
        let key = EncodedRecordKey::from_key(&value.key())?;
        let mutation_key = MutationKey {
            family: descriptor.family.clone(),
            key: key.clone(),
        };
        let value = erase_record(descriptor.clone(), value);
        self.add(
            mutation_key,
            RecordMutation::Replace {
                descriptor,
                type_id: TypeId::of::<R>(),
                key,
                expected: expected.encode_version(),
                value,
            },
        )
    }

    /// Retains a typed compare-and-remove for later atomic validation.
    pub fn remove<R: StoredRecord>(
        &mut self,
        key: R::Key,
        expected: R::Version,
    ) -> Result<(), ZapError> {
        let descriptor = R::descriptor()?;
        let key = EncodedRecordKey::from_key(&key)?;
        let mutation_key = MutationKey {
            family: descriptor.family.clone(),
            key: key.clone(),
        };
        self.add(
            mutation_key,
            RecordMutation::Remove {
                descriptor,
                type_id: TypeId::of::<R>(),
                key,
                expected: expected.encode_version(),
            },
        )
    }

    /// Adds one invariant that the commit validator must witness.
    pub fn require(&mut self, requirement: RequirementRef) {
        if !self.assertions.contains(&requirement) {
            self.assertions.push(requirement);
            self.assertions.sort();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    pub(crate) fn prepare(
        &self,
        records: &RecordSet,
    ) -> Result<Vec<PreparedRecordMutation>, ZapError> {
        let mut prepared = Vec::with_capacity(self.mutations.len());
        for mutation in self.mutations.values() {
            let row = match mutation {
                RecordMutation::Insert {
                    descriptor,
                    type_id,
                    key,
                    value,
                } => {
                    validate_registration(records, descriptor, *type_id)?;
                    PreparedRecordMutation {
                        kind: MutationKind::Insert,
                        descriptor: descriptor.clone(),
                        key: key.clone(),
                        expected_version: None,
                        new_version: Some(value.version_bytes()),
                        value: Some(value.value_bytes()?),
                    }
                }
                RecordMutation::Replace {
                    descriptor,
                    type_id,
                    key,
                    expected,
                    value,
                } => {
                    validate_registration(records, descriptor, *type_id)?;
                    PreparedRecordMutation {
                        kind: MutationKind::Replace,
                        descriptor: descriptor.clone(),
                        key: key.clone(),
                        expected_version: Some(expected.clone()),
                        new_version: Some(value.version_bytes()),
                        value: Some(value.value_bytes()?),
                    }
                }
                RecordMutation::Remove {
                    descriptor,
                    type_id,
                    key,
                    expected,
                } => {
                    validate_registration(records, descriptor, *type_id)?;
                    PreparedRecordMutation {
                        kind: MutationKind::Remove,
                        descriptor: descriptor.clone(),
                        key: key.clone(),
                        expected_version: Some(expected.clone()),
                        new_version: None,
                        value: None,
                    }
                }
            };
            prepared.push(row);
        }
        Ok(prepared)
    }

    fn add(&mut self, key: MutationKey, mutation: RecordMutation) -> Result<(), ZapError> {
        if self.mutations.insert(key, mutation).is_some() {
            return Err(ZapError::from_static(
                ErrorCode::Conflict,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
                "one ChangeSet cannot contain conflicting mutations for the same record",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        Ok(())
    }
}

fn validate_registration(
    records: &RecordSet,
    descriptor: &RecordDescriptor,
    type_id: TypeId,
) -> Result<(), ZapError> {
    if records.registration_matches(&descriptor.family, type_id, descriptor) {
        Ok(())
    } else {
        Err(ZapError::from_static(
            ErrorCode::InternalInvariant,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
            "ChangeSet record type or descriptor is not present in the fixed RecordSet",
            FixSurface::Configuration,
            ErrorDetail::None,
        ))
    }
}

pub(crate) fn effect_mutation_digest(
    changes: &ChangeSet,
    records: &RecordSet,
    allowed: &[RecordFamily],
) -> Result<zap_wire::EffectMutationDigest, ZapError> {
    #[derive(Serialize)]
    struct MutationDigestRow<'a> {
        family: &'a RecordFamily,
        key: &'a [u8],
        kind: &'static str,
        expected_version: Option<&'a [u8]>,
        new_version: Option<&'a [u8]>,
        value: Option<&'a [u8]>,
    }
    let prepared = changes.prepare(records)?;
    if prepared
        .iter()
        .any(|row| allowed.binary_search(&row.descriptor().family).is_err())
    {
        return Err(ZapError::from_static(
            ErrorCode::Conflict,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE",
            "effect mutation writes an undeclared record family",
            FixSurface::Configuration,
            ErrorDetail::None,
        ));
    }
    let rows = prepared
        .iter()
        .map(|row| MutationDigestRow {
            family: &row.descriptor().family,
            key: row.key().as_bytes(),
            kind: match row.kind() {
                MutationKind::Insert => "insert",
                MutationKind::Replace => "replace",
                MutationKind::Remove => "remove",
            },
            expected_version: row.expected_version(),
            new_version: row.new_version(),
            value: row.value(),
        })
        .collect::<Vec<_>>();
    let bytes = zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, &rows)?;
    Ok(zap_wire::EffectMutationDigest::hash(bytes.as_bytes()))
}

struct OverlayMutation {
    kind: MutationKind,
    descriptor: RecordDescriptor,
    key: EncodedRecordKey,
    prior: Option<Arc<dyn ErasedRecord>>,
    value: Option<Arc<dyn ErasedRecord>>,
}

pub(crate) struct ChangeSetOverlay<'a> {
    base: &'a dyn crate::StateReader,
    mutations: BTreeMap<(RecordFamily, EncodedRecordKey), OverlayMutation>,
    indexes: index_overlay::IndexOverlayCache,
}

impl<'a> ChangeSetOverlay<'a> {
    pub(crate) fn new(
        base: &'a dyn crate::StateReader,
        changes: &ChangeSet,
        records: &RecordSet,
        allowed: &[RecordFamily],
    ) -> Result<Self, ZapError> {
        let prepared = changes.prepare(records)?;
        let mut mutations = BTreeMap::new();
        for row in prepared {
            if allowed.binary_search(&row.descriptor().family).is_err() {
                return Err(overlay_conflict());
            }
            let current = base.get_erased(&row.descriptor().family, row.key())?;
            match row.kind() {
                MutationKind::Insert if current.is_some() => return Err(overlay_conflict()),
                MutationKind::Replace | MutationKind::Remove => {
                    let current = current.as_ref().ok_or_else(overlay_conflict)?;
                    if Some(current.version_bytes().as_slice()) != row.expected_version() {
                        return Err(overlay_conflict());
                    }
                }
                MutationKind::Insert => {}
            }
            let value = row
                .value()
                .map(|value| {
                    let payload = zap_wire::CanonicalPayload::from_canonical_json(
                        row.descriptor().value_codec,
                        value,
                    )?;
                    records.decode(&row.descriptor().family, &payload)
                })
                .transpose()?;
            let key = (row.descriptor().family.clone(), row.key().clone());
            if mutations
                .insert(
                    key,
                    OverlayMutation {
                        kind: row.kind(),
                        descriptor: row.descriptor().clone(),
                        key: row.key().clone(),
                        prior: current,
                        value,
                    },
                )
                .is_some()
            {
                return Err(overlay_conflict());
            }
        }
        Ok(Self {
            base,
            mutations,
            indexes: index_overlay::IndexOverlayCache::default(),
        })
    }
}

impl crate::StateReader for ChangeSetOverlay<'_> {
    fn identity(&self) -> crate::StoreIdentity {
        self.base.identity()
    }

    fn revision(&self) -> zap_wire::Revision {
        self.base.revision()
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        match self.mutations.get(&(family.clone(), key.clone())) {
            Some(OverlayMutation {
                kind: MutationKind::Remove,
                ..
            }) => Ok(None),
            Some(mutation) => Ok(mutation.value.clone()),
            None => self.base.get_erased(family, key),
        }
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: crate::EncodedKeyRange,
        limit: crate::PageLimit,
    ) -> Result<crate::ErasedRecordPage, ZapError> {
        record_overlay::scan(self, family, range, limit)
    }

    fn scan_index(&self, request: &crate::IndexScanRequest) -> Result<crate::IndexPage, ZapError> {
        index_overlay::scan(self, request)
    }
}

fn overlay_conflict() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE",
        "effect simulation overlay conflicts with the transaction pre-state",
        FixSurface::Store,
        ErrorDetail::None,
    )
}
