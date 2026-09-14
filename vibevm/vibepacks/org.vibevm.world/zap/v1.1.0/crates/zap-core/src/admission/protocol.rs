use std::sync::Arc;

use serde::{Serialize, de::DeserializeOwned};
use specmark::spec;
use zap_wire::{
    ActionClass, ActionImpactDigest, AdmissionId, CanonicalOutput, CanonicalPayload, CodecEpoch,
    CommandDigest, CommandHeader, EffectMutationDigest, EffectPreflightDigest, HoldId,
    PayloadDigest, ReducerEpoch, RelevantBasisDigest, ZapError,
};

use crate::{
    ActorRef, AffectedScopeRequest, AffectedScopeView, BasisRequest, EffectBundlePreflightView,
    EffectBundleRequest, IndependenceRequest, IndependenceView, IndependenceWitness, IndexFamily,
    RecordFamily, RelevantBasis, SafeJobRequest, SafeJobView, SafeJobWitness, StateReader,
};

use super::ActionImpactView;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionBasis {
    pub request: BasisRequest,
    pub relevant: RelevantBasis,
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionAdmissionRequest {
    pub action: ActionClass,
    pub header: CommandHeader,
    pub command_digest: CommandDigest,
    pub payload_digest: PayloadDigest,
    pub basis: Option<ActionBasis>,
    pub impact: ActionImpactView,
    pub impact_request: super::ActionImpactRequest,
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionAdmissionNeeds {
    selected_effect: Option<EffectBundleRequest>,
    affected_scopes: Vec<AffectedScopeRequest>,
    independence: Vec<IndependenceRequest>,
    safe_jobs: Vec<SafeJobRequest>,
}

impl ActionAdmissionNeeds {
    pub fn new(
        selected_effect: Option<EffectBundleRequest>,
        mut affected_scopes: Vec<AffectedScopeRequest>,
        mut independence: Vec<IndependenceRequest>,
        mut safe_jobs: Vec<SafeJobRequest>,
    ) -> Result<Self, ZapError> {
        affected_scopes.sort_by_key(AffectedScopeRequest::request_digest);
        independence.sort_by_key(IndependenceRequest::request_digest);
        safe_jobs.sort_by_key(SafeJobRequest::request_digest);
        if has_duplicate_digest(&affected_scopes, AffectedScopeRequest::request_digest)
            || has_duplicate_digest(&independence, IndependenceRequest::request_digest)
            || has_duplicate_digest(&safe_jobs, SafeJobRequest::request_digest)
        {
            return Err(admission_error(
                "admission needs contain duplicate requests",
            ));
        }
        Ok(Self {
            selected_effect,
            affected_scopes,
            independence,
            safe_jobs,
        })
    }
    pub fn selected_effect(&self) -> Option<&EffectBundleRequest> {
        self.selected_effect.as_ref()
    }
    pub fn affected_scopes(&self) -> &[AffectedScopeRequest] {
        &self.affected_scopes
    }
    pub fn independence(&self) -> &[IndependenceRequest] {
        &self.independence
    }
    pub fn safe_jobs(&self) -> &[SafeJobRequest] {
        &self.safe_jobs
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionAdmissionPreflightRecord {
    pub selected_effect: Option<EffectBundlePreflightView>,
    pub affected_scopes: Vec<AffectedScopeView>,
    pub independence: Vec<IndependenceView>,
    pub safe_jobs: Vec<SafeJobView>,
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionAdmissionPreflight<'a> {
    record: &'a ActionAdmissionPreflightRecord,
    independence: &'a [IndependenceWitness],
    safe_jobs: &'a [SafeJobWitness],
    transaction_seal: &'a Arc<()>,
    service_seal: &'a Arc<()>,
}

impl<'a> ActionAdmissionPreflight<'a> {
    pub(crate) fn new(
        record: &'a ActionAdmissionPreflightRecord,
        independence: &'a [IndependenceWitness],
        safe_jobs: &'a [SafeJobWitness],
        transaction_seal: &'a Arc<()>,
        service_seal: &'a Arc<()>,
    ) -> Self {
        Self {
            record,
            independence,
            safe_jobs,
            transaction_seal,
            service_seal,
        }
    }
    pub fn record(&self) -> &ActionAdmissionPreflightRecord {
        self.record
    }
    pub fn selected_effect(&self) -> Option<&EffectBundlePreflightView> {
        self.record.selected_effect.as_ref()
    }
    pub fn affected_scope(&self, request_digest: PayloadDigest) -> Option<&AffectedScopeView> {
        self.record
            .affected_scopes
            .iter()
            .find(|view| view.request_digest == request_digest)
    }
    pub fn independence_for(
        &self,
        hold_id: &HoldId,
        candidate_request_digest: PayloadDigest,
    ) -> Option<&IndependenceWitness> {
        self.independence.iter().find(|witness| {
            witness.view().hold_id == *hold_id
                && witness.view().candidate_request_digest == candidate_request_digest
                && Arc::ptr_eq(&witness.transaction_seal, self.transaction_seal)
                && Arc::ptr_eq(&witness.service_seal, self.service_seal)
        })
    }
    pub fn safe_jobs_for(&self, hold_id: &HoldId) -> Option<&SafeJobWitness> {
        self.safe_jobs.iter().find(|witness| {
            witness.view().hold_id == *hold_id
                && Arc::ptr_eq(&witness.transaction_seal, self.transaction_seal)
                && Arc::ptr_eq(&witness.service_seal, self.service_seal)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub enum ActionAdmissionBasis {
    Exempt {
        impact: ActionImpactDigest,
    },
    Economic {
        admission_id: AdmissionId,
        impact: ActionImpactDigest,
        selected_effect: EffectPreflightDigest,
    },
}

impl ActionAdmissionBasis {
    pub const fn impact(&self) -> ActionImpactDigest {
        match self {
            Self::Exempt { impact } | Self::Economic { impact, .. } => *impact,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct AdmissionMutationScope {
    affected_records: Vec<RecordFamily>,
    affected_indexes: Vec<IndexFamily>,
}

impl AdmissionMutationScope {
    pub fn new(
        mut affected_records: Vec<RecordFamily>,
        mut affected_indexes: Vec<IndexFamily>,
    ) -> Result<Self, ZapError> {
        affected_records.sort();
        affected_indexes.sort();
        if has_duplicates(&affected_records) || has_duplicates(&affected_indexes) {
            return Err(admission_error(
                "admission mutation scope contains duplicates",
            ));
        }
        Ok(Self {
            affected_records,
            affected_indexes,
        })
    }
    pub fn affected_records(&self) -> &[RecordFamily] {
        &self.affected_records
    }
    pub fn affected_indexes(&self) -> &[IndexFamily] {
        &self.affected_indexes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct AdmissionHookDescriptor {
    id: crate::CapabilityId,
    reducer_epoch: ReducerEpoch,
    exempt_scope: AdmissionMutationScope,
    economic_scope: AdmissionMutationScope,
}

impl AdmissionHookDescriptor {
    pub fn new(
        id: crate::CapabilityId,
        reducer_epoch: ReducerEpoch,
        exempt_scope: AdmissionMutationScope,
        economic_scope: AdmissionMutationScope,
    ) -> Result<Self, ZapError> {
        Ok(Self {
            id,
            reducer_epoch,
            exempt_scope,
            economic_scope,
        })
    }
    pub fn id(&self) -> &crate::CapabilityId {
        &self.id
    }
    pub const fn reducer_epoch(&self) -> ReducerEpoch {
        self.reducer_epoch
    }
    pub fn scope_for(&self, basis: &ActionAdmissionBasis) -> &AdmissionMutationScope {
        match basis {
            ActionAdmissionBasis::Exempt { .. } => &self.exempt_scope,
            ActionAdmissionBasis::Economic { .. } => &self.economic_scope,
        }
    }

    pub(crate) fn scopes(&self) -> [&AdmissionMutationScope; 2] {
        [&self.exempt_scope, &self.economic_scope]
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionAdmissionObservation {
    pub hook_id: crate::CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub basis: ActionAdmissionBasis,
    pub impact: ActionImpactView,
    pub payload: Vec<u8>,
    pub payload_digest: PayloadDigest,
}

impl ActionAdmissionObservation {
    pub fn new<T: Serialize>(
        descriptor: &AdmissionHookDescriptor,
        basis: ActionAdmissionBasis,
        impact: ActionImpactView,
        value: &T,
    ) -> Result<Self, ZapError> {
        if basis.impact() != impact.digest {
            return Err(admission_error("admission basis and impact digest differ"));
        }
        let payload = CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?;
        Ok(Self {
            hook_id: descriptor.id().clone(),
            reducer_epoch: descriptor.reducer_epoch(),
            basis,
            impact,
            payload: payload.as_bytes().to_vec(),
            payload_digest: payload.digest(),
        })
    }
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ZapError> {
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &self.payload)?;
        if payload.digest() != self.payload_digest {
            return Err(admission_error(
                "admission observation payload digest differs",
            ));
        }
        payload.decode_json()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-admission")]
pub struct ActionProductOutcome {
    pub selected_effect: Option<EffectPreflightDigest>,
    pub mutation_digest: EffectMutationDigest,
    pub relevant_after: Option<RelevantBasisDigest>,
}

/// Owns the complete privileged admission lifecycle around one product write.
///
/// ```
/// use zap_core::ActionAdmissionProvider;
/// fn registered(provider: &dyn ActionAdmissionProvider) {
///     assert!(!provider.descriptor().id().as_str().is_empty());
/// }
/// ```
pub trait ActionAdmissionProvider: Send + Sync + 'static {
    fn descriptor(&self) -> &AdmissionHookDescriptor;
    fn needs(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError>;
    fn admit(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
        preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError>;
    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        changes: &mut crate::ChangeSet,
    ) -> Result<(), ZapError>;
    fn verify_after(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        outcome: &ActionProductOutcome,
    ) -> Result<(), ZapError>;
}

fn has_duplicate_digest<T>(values: &[T], digest: impl Fn(&T) -> PayloadDigest) -> bool {
    values
        .windows(2)
        .any(|pair| digest(&pair[0]) == digest(&pair[1]))
}

fn has_duplicates<T: Eq>(values: &[T]) -> bool {
    values.windows(2).any(|pair| pair[0] == pair[1])
}

fn admission_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT",
        message,
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
