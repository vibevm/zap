use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use specmark::spec;
use zap_wire::{
    ActionExceptionId, AdmissionId, AssumptionId, AttemptId, AuthorizationRef, BaseId, BundleId,
    CampaignId, CandidateId, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CapabilityObservationId, ChangeAlternativeId, ChangeAssessmentId, ChangeBaselineId, ChangeId,
    CharterId, ClosureId, CodecEpoch, CommandId, CompletionProviderId, ConditionId, ContractId,
    ControllerId, CostForecastId, CredentialId, DecisionId, DeferralId, DispatchId, DreamId,
    EffectId, EncounterId, ErrorCode, ErrorDetail, EventId, EvidenceId, FactId, FixSurface, ForkId,
    GoalId, HarnessId, HoldId, InformationOpportunityId, InformationSelectionId,
    IntegrationAcceptanceId, IntentId, JobId, LoweringId, MessageId, MilestoneAchievementId,
    MilestoneId, MilestoneRevisionId, ObligationId, ObservationRef, OperationId, OutcomeId,
    PacketId, PauseId, PolicyId, PrincipalId, ProblemId, PromotionId, QueryId,
    ReconciliationRequestId, ResourceId, ReviewId, Revision, RiskId, SemanticRequestId, SourceId,
    StageAcceptanceId, StopRuleId, StoreId, StrategicRevisionId, TransactionId, VerificationId,
    WaitId, WorkAcceptanceId, WorkId, ZapError,
};

use crate::{EncodedRecordKey, IndexFamily, RecordDescriptor, RecordFamily};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

/// A typed record key with one canonical binary encoding.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
/// Encodes a key for ordered storage.
///
/// ```
/// use zap_core::RecordKey;
/// let key = zap_wire::WorkId::parse("work.docs")?;
/// assert_eq!(key.encode_key()?, b"work.docs");
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
pub trait RecordKey: Clone + Eq + Ord + Send + Sync + 'static {
    fn encode_key(&self) -> Result<Vec<u8>, ZapError>;
}

/// A typed record version whose equality is the replacement contract.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
)]
/// Encodes the replacement version deterministically.
///
/// ```
/// use zap_core::VersionStamp;
/// let encoded = zap_wire::Revision::new(7).encode_version();
/// assert_eq!(encoded, 7_u64.to_be_bytes());
/// ```
pub trait VersionStamp: Clone + Eq + Send + Sync + 'static {
    fn encode_version(&self) -> Vec<u8>;
}

impl VersionStamp for Revision {
    fn encode_version(&self) -> Vec<u8> {
        self.get().to_be_bytes().to_vec()
    }
}

/// An open typed record contract that grants no access without registration.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
/// Defines one typed record family and its replacement version.
///
/// ```
/// use zap_core::StoredRecord;
/// fn identity<R: StoredRecord>(record: &R) -> Result<(Vec<u8>, Vec<u8>), zap_wire::ZapError> {
///     use zap_core::{RecordKey, VersionStamp};
///     Ok((record.key().encode_key()?, record.version().encode_version()))
/// }
/// ```
pub trait StoredRecord: CanonicalEncode + CanonicalDecode + Clone + Send + Sync + 'static {
    type Key: RecordKey;
    type Version: VersionStamp;
    const FAMILY: &'static str;

    fn key(&self) -> Self::Key;
    fn version(&self) -> Self::Version;
    fn descriptor() -> Result<RecordDescriptor, ZapError>;

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError> {
        Ok(Vec::new())
    }
}

/// A canonical row contributed by one registered typed record.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub struct RecordIndexRow {
    family: IndexFamily,
    key: Vec<u8>,
    value: Vec<u8>,
}

impl RecordIndexRow {
    pub fn new<K: Serialize, V: Serialize>(
        family: IndexFamily,
        key: &K,
        value: &V,
    ) -> Result<Self, ZapError> {
        let key = CanonicalOutput::encode_json(CodecEpoch::CURRENT, key)?
            .as_bytes()
            .to_vec();
        let value = CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
            .as_bytes()
            .to_vec();
        if key.is_empty() || key.len() > 4096 || value.len() > 65_536 {
            return Err(invalid_index_row());
        }
        Ok(Self { family, key, value })
    }

    pub fn partitioned<P: Serialize, S: Serialize, V: Serialize>(
        family: IndexFamily,
        partition: &P,
        suffix: &S,
        value: &V,
    ) -> Result<Self, ZapError> {
        let partition = crate::IndexPartition::new(partition)?;
        let key = crate::index_scan::partitioned_key(&partition, suffix)?;
        let value = CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
            .as_bytes()
            .to_vec();
        if value.is_empty() || value.len() > 65_536 {
            return Err(invalid_index_row());
        }
        Ok(Self { family, key, value })
    }

    pub fn family(&self) -> &IndexFamily {
        &self.family
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

/// A registry-decoded record with checked concrete type metadata.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
/// Exposes canonical bytes only after registry-controlled type erasure.
///
/// ```
/// use zap_core::ErasedRecord;
/// fn exact_bytes(record: &dyn ErasedRecord) -> Result<(Vec<u8>, Vec<u8>), zap_wire::ZapError> {
///     let key = record.key_bytes()?;
///     let value = record.value_bytes()?;
///     assert!(!key.is_empty());
///     Ok((key, value))
/// }
/// ```
pub trait ErasedRecord: Any + Send + Sync {
    fn descriptor(&self) -> &RecordDescriptor;
    fn as_any(&self) -> &dyn Any;
    fn key_bytes(&self) -> Result<Vec<u8>, ZapError>;
    fn version_bytes(&self) -> Vec<u8>;
    fn value_bytes(&self) -> Result<Vec<u8>, ZapError>;
    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError>;
}

struct RegisteredRecord<R> {
    descriptor: RecordDescriptor,
    value: R,
}

pub(crate) fn erase_record<R: StoredRecord>(
    descriptor: RecordDescriptor,
    value: R,
) -> Arc<dyn ErasedRecord> {
    Arc::new(RegisteredRecord { descriptor, value })
}

impl<R: StoredRecord> ErasedRecord for RegisteredRecord<R> {
    fn descriptor(&self) -> &RecordDescriptor {
        &self.descriptor
    }

    fn as_any(&self) -> &dyn Any {
        &self.value
    }

    fn key_bytes(&self) -> Result<Vec<u8>, ZapError> {
        self.value.key().encode_key()
    }

    fn version_bytes(&self) -> Vec<u8> {
        self.value.version().encode_version()
    }

    fn value_bytes(&self) -> Result<Vec<u8>, ZapError> {
        Ok(self
            .value
            .encode_canonical(self.descriptor.value_codec)?
            .as_bytes()
            .to_vec())
    }

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError> {
        self.value.index_rows()
    }
}

type DecodeRecord =
    fn(&RecordDescriptor, &CanonicalPayload) -> Result<Arc<dyn ErasedRecord>, ZapError>;

#[derive(Clone)]
struct RecordRegistration {
    descriptor: RecordDescriptor,
    type_id: TypeId,
    decode: DecodeRecord,
}

fn decode_record<R: StoredRecord>(
    descriptor: &RecordDescriptor,
    payload: &CanonicalPayload,
) -> Result<Arc<dyn ErasedRecord>, ZapError> {
    let value = R::decode_canonical(payload)?;
    Ok(erase_record(descriptor.clone(), value))
}

fn registration<R: StoredRecord>() -> Result<RecordRegistration, ZapError> {
    let descriptor = R::descriptor()?;
    let declared = RecordFamily::parse(R::FAMILY)?;
    if descriptor.family != declared {
        return Err(registry_invariant());
    }
    Ok(RecordRegistration {
        descriptor,
        type_id: TypeId::of::<R>(),
        decode: decode_record::<R>,
    })
}

fn registry_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "record registration type, family or codec does not match",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn duplicate_family() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "record family is registered more than once",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn invalid_index_row() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "record index keys and values must be bounded canonical data",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

/// A duplicate-free registry of concrete record families and codecs.
#[derive(Clone, Default)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-contracts")]
pub struct RecordSet {
    registrations: BTreeMap<RecordFamily, RecordRegistration>,
}

impl RecordSet {
    /// Creates a lawful empty record registry.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates a registry containing one checked concrete record type.
    pub fn single<R: StoredRecord>() -> Result<Self, ZapError> {
        let mut result = Self::empty();
        result.register::<R>()?;
        Ok(result)
    }

    /// Registers one concrete record type exactly once.
    pub fn register<R: StoredRecord>(&mut self) -> Result<(), ZapError> {
        let registration = registration::<R>()?;
        if self
            .registrations
            .insert(registration.descriptor.family.clone(), registration)
            .is_some()
        {
            return Err(duplicate_family());
        }
        Ok(())
    }

    /// Composes whole feature registries and rejects duplicate families.
    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for (family, registration) in set.registrations {
                if result.registrations.insert(family, registration).is_some() {
                    return Err(duplicate_family());
                }
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    pub fn families(&self) -> impl Iterator<Item = &RecordFamily> {
        self.registrations.keys()
    }

    pub fn descriptor(&self, family: &RecordFamily) -> Option<&RecordDescriptor> {
        self.registrations.get(family).map(|row| &row.descriptor)
    }

    pub fn type_matches<R: StoredRecord>(&self) -> bool {
        self.registrations
            .values()
            .any(|row| row.type_id == TypeId::of::<R>())
    }

    pub(crate) fn registration_matches(
        &self,
        family: &RecordFamily,
        type_id: TypeId,
        descriptor: &RecordDescriptor,
    ) -> bool {
        self.registrations
            .get(family)
            .is_some_and(|row| row.type_id == type_id && &row.descriptor == descriptor)
    }

    pub fn decode(
        &self,
        family: &RecordFamily,
        payload: &CanonicalPayload,
    ) -> Result<Arc<dyn ErasedRecord>, ZapError> {
        let registration = self
            .registrations
            .get(family)
            .ok_or_else(registry_invariant)?;
        (registration.decode)(&registration.descriptor, payload)
    }
}

/// The continuation state of a raw record scan without public query metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#immutable-record-reads"
)]
pub enum RecordCompleteness {
    Complete,
    More,
    UnknownBoundary,
}

/// A bounded erased record page returned by StateReader.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#immutable-record-reads"
)]
pub struct ErasedRecordPage {
    pub items: Vec<Arc<dyn ErasedRecord>>,
    pub completeness: RecordCompleteness,
    pub last_key: Option<EncodedRecordKey>,
}

/// A bounded typed record scan without public query metadata.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#immutable-record-reads"
)]
pub struct RecordPage<R> {
    pub items: Vec<R>,
    pub completeness: RecordCompleteness,
    pub last_key: Option<EncodedRecordKey>,
}

macro_rules! id_record_keys {
    ($($name:ty),+ $(,)?) => {
        $(
            impl RecordKey for $name {
                fn encode_key(&self) -> Result<Vec<u8>, ZapError> {
                    Ok(self.as_str().as_bytes().to_vec())
                }
            }
        )+
    };
}

id_record_keys!(
    CampaignId,
    StoreId,
    BaseId,
    CommandId,
    EventId,
    TransactionId,
    ChangeId,
    IntentId,
    OutcomeId,
    ObligationId,
    WorkId,
    ContractId,
    ReviewId,
    DecisionId,
    SourceId,
    EvidenceId,
    DeferralId,
    LoweringId,
    StrategicRevisionId,
    DreamId,
    PacketId,
    JobId,
    AttemptId,
    VerificationId,
    HoldId,
    PauseId,
    HarnessId,
    CapabilityObservationId,
    GoalId,
    CharterId,
    PolicyId,
    ControllerId,
    CredentialId,
    SemanticRequestId,
    EffectId,
    BundleId,
    EncounterId,
    CandidateId,
    ChangeAssessmentId,
    ChangeBaselineId,
    ChangeAlternativeId,
    CostForecastId,
    ProblemId,
    ActionExceptionId,
    StopRuleId,
    DispatchId,
    ForkId,
    RiskId,
    ConditionId,
    ResourceId,
    WaitId,
    QueryId,
    AssumptionId,
    StageAcceptanceId,
    IntegrationAcceptanceId,
    WorkAcceptanceId,
    ClosureId,
    PromotionId,
    FactId,
    PrincipalId,
    OperationId,
    AdmissionId,
    CompletionProviderId,
    AuthorizationRef,
    ObservationRef,
    MessageId,
    ReconciliationRequestId,
    MilestoneId,
    MilestoneRevisionId,
    MilestoneAchievementId,
    InformationOpportunityId,
    InformationSelectionId,
);
