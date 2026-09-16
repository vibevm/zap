use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ArtifactDigest, CanonicalOutput, CanonicalPayload, ChangeAlternativeId, CodecEpoch, EffectId,
    EffectItemDigest, EffectMutationDigest, EffectPreflightDigest, EventId, EventKind,
    PayloadDigest, ReducerEpoch, RelevantBasisDigest, Revision, SubjectRef, WorkId, ZapError,
};

use crate::{ActorRef, BasisRequest, ChangeSet, CommandPayload, StateReader, StoreIdentity};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE");

mod draft;

pub use draft::{EffectBundleDraft, EffectComparisonDraft, EffectDraft};

#[cfg(test)]
#[path = "effects/tests.rs"]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectPreflightRequest {
    effect_id: EffectId,
    index: u32,
    kind: EventKind,
    payload: CanonicalPayload,
    predecessors: Vec<EffectId>,
    product_event_id: EventId,
    basis: BasisRequest,
    declared_subjects: Vec<SubjectRef>,
    relevant_before: RelevantBasisDigest,
    declared_relevant_after: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectPreflightRequestInput {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload: CanonicalPayload,
    pub predecessors: Vec<EffectId>,
    pub product_event_id: EventId,
    pub basis: BasisRequest,
    pub declared_subjects: Vec<SubjectRef>,
    pub relevant_before: RelevantBasisDigest,
    pub declared_relevant_after: RelevantBasisDigest,
}

impl EffectPreflightRequest {
    pub fn new(mut input: EffectPreflightRequestInput) -> Result<Self, ZapError> {
        input.predecessors.sort();
        input.predecessors.dedup();
        input.declared_subjects.sort();
        input.declared_subjects.dedup();
        if input.declared_subjects.is_empty() {
            return Err(effect_error(
                "effect preflight requires a declared typed subject",
            ));
        }
        Ok(Self {
            effect_id: input.effect_id,
            index: input.index,
            kind: input.kind,
            payload: input.payload,
            predecessors: input.predecessors,
            product_event_id: input.product_event_id,
            basis: input.basis,
            declared_subjects: input.declared_subjects,
            relevant_before: input.relevant_before,
            declared_relevant_after: input.declared_relevant_after,
        })
    }
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }
    pub fn payload(&self) -> &CanonicalPayload {
        &self.payload
    }
    pub fn predecessors(&self) -> &[EffectId] {
        &self.predecessors
    }
    pub fn product_event_id(&self) -> &EventId {
        &self.product_event_id
    }
    pub fn basis(&self) -> &BasisRequest {
        &self.basis
    }
    pub fn declared_subjects(&self) -> &[SubjectRef] {
        &self.declared_subjects
    }
    pub const fn relevant_before(&self) -> RelevantBasisDigest {
        self.relevant_before
    }
    pub const fn declared_relevant_after(&self) -> RelevantBasisDigest {
        self.declared_relevant_after
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectBundleRequest {
    alternative_id: ChangeAlternativeId,
    committed_prefix: Vec<EffectId>,
    initial_basis: RelevantBasisDigest,
    effects: Vec<EffectPreflightRequest>,
    no_op_basis: Option<BasisRequest>,
    request_digest: PayloadDigest,
}

impl EffectBundleRequest {
    pub fn new(
        alternative_id: ChangeAlternativeId,
        committed_prefix: Vec<EffectId>,
        initial_basis: RelevantBasisDigest,
        effects: Vec<EffectPreflightRequest>,
    ) -> Result<Self, ZapError> {
        if effects.is_empty() || has_duplicates(&committed_prefix) {
            return Err(effect_error(
                "nonempty effect request or unique committed prefix is required",
            ));
        }
        let mut prior = committed_prefix.clone();
        for (offset, effect) in effects.iter().enumerate() {
            if effect.index as usize != committed_prefix.len() + offset
                || prior.contains(&effect.effect_id)
                || effect.predecessors.iter().any(|id| !prior.contains(id))
            {
                return Err(effect_error(
                    "effect bundle order, prefix or predecessor closure is invalid",
                ));
            }
            prior.push(effect.effect_id.clone());
        }
        #[derive(Serialize)]
        struct DigestBody<'a> {
            alternative_id: &'a ChangeAlternativeId,
            committed_prefix: &'a [EffectId],
            initial_basis: RelevantBasisDigest,
            effects: Vec<EffectRequestDigest<'a>>,
            no_op_basis: Option<&'a BasisRequest>,
        }
        #[derive(Serialize)]
        struct EffectRequestDigest<'a> {
            effect_id: &'a EffectId,
            index: u32,
            kind: &'a EventKind,
            payload_digest: PayloadDigest,
            predecessors: &'a [EffectId],
            product_event_id: &'a EventId,
            basis: &'a BasisRequest,
            subjects: &'a [SubjectRef],
            before: RelevantBasisDigest,
            after: RelevantBasisDigest,
        }
        let body = DigestBody {
            alternative_id: &alternative_id,
            committed_prefix: &committed_prefix,
            initial_basis,
            effects: effects
                .iter()
                .map(|effect| EffectRequestDigest {
                    effect_id: &effect.effect_id,
                    index: effect.index,
                    kind: &effect.kind,
                    payload_digest: effect.payload.digest(),
                    predecessors: &effect.predecessors,
                    product_event_id: &effect.product_event_id,
                    basis: &effect.basis,
                    subjects: &effect.declared_subjects,
                    before: effect.relevant_before,
                    after: effect.declared_relevant_after,
                })
                .collect(),
            no_op_basis: None,
        };
        let request_digest = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &body)?.digest();
        Ok(Self {
            alternative_id,
            committed_prefix,
            initial_basis,
            effects,
            no_op_basis: None,
            request_digest,
        })
    }

    pub fn new_no_op(
        alternative_id: ChangeAlternativeId,
        committed_prefix: Vec<EffectId>,
        initial_basis: RelevantBasisDigest,
        no_op_basis: BasisRequest,
    ) -> Result<Self, ZapError> {
        if has_duplicates(&committed_prefix) {
            return Err(effect_error("committed effect prefix contains a duplicate"));
        }
        #[derive(Serialize)]
        struct DigestBody<'a> {
            alternative_id: &'a ChangeAlternativeId,
            committed_prefix: &'a [EffectId],
            initial_basis: RelevantBasisDigest,
            effects: [u8; 0],
            no_op_basis: &'a BasisRequest,
        }
        let body = DigestBody {
            alternative_id: &alternative_id,
            committed_prefix: &committed_prefix,
            initial_basis,
            effects: [],
            no_op_basis: &no_op_basis,
        };
        let request_digest = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &body)?.digest();
        Ok(Self {
            alternative_id,
            committed_prefix,
            initial_basis,
            effects: Vec::new(),
            no_op_basis: Some(no_op_basis),
            request_digest,
        })
    }
    pub fn alternative_id(&self) -> &ChangeAlternativeId {
        &self.alternative_id
    }
    pub fn effects(&self) -> &[EffectPreflightRequest] {
        &self.effects
    }
    pub fn committed_prefix(&self) -> &[EffectId] {
        &self.committed_prefix
    }
    pub fn no_op_basis(&self) -> Option<&BasisRequest> {
        self.no_op_basis.as_ref()
    }
    pub const fn initial_basis(&self) -> RelevantBasisDigest {
        self.initial_basis
    }
    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-contracts")]
pub struct EffectScope {
    basis: BasisRequest,
    work_ids: Vec<WorkId>,
    subjects: Vec<SubjectRef>,
    artifacts: Vec<ArtifactDigest>,
}

impl EffectScope {
    pub fn new(
        basis: BasisRequest,
        mut work_ids: Vec<WorkId>,
        mut subjects: Vec<SubjectRef>,
        mut artifacts: Vec<ArtifactDigest>,
    ) -> Result<Self, ZapError> {
        sort_unique(&mut work_ids);
        sort_unique(&mut subjects);
        sort_unique(&mut artifacts);
        if work_ids.is_empty() && subjects.is_empty() {
            return Err(effect_error(
                "effect scope must name typed work or subjects",
            ));
        }
        Ok(Self {
            basis,
            work_ids,
            subjects,
            artifacts,
        })
    }
    pub fn basis(&self) -> &BasisRequest {
        &self.basis
    }
    pub fn work_ids(&self) -> &[WorkId] {
        &self.work_ids
    }
    pub fn subjects(&self) -> &[SubjectRef] {
        &self.subjects
    }
    pub fn artifacts(&self) -> &[ArtifactDigest] {
        &self.artifacts
    }
}

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-contracts")]
pub struct EffectScopeContext {
    store: StoreIdentity,
    actor: Option<ActorRef>,
    effect_id: EffectId,
    product_event_id: EventId,
    observed_revision: Revision,
}

impl EffectScopeContext {
    pub(crate) fn new(
        store: StoreIdentity,
        actor: Option<ActorRef>,
        effect_id: EffectId,
        product_event_id: EventId,
        observed_revision: Revision,
    ) -> Self {
        Self {
            store,
            actor,
            effect_id,
            product_event_id,
            observed_revision,
        }
    }
    pub fn store(&self) -> &StoreIdentity {
        &self.store
    }
    pub fn actor(&self) -> Option<&ActorRef> {
        self.actor.as_ref()
    }
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }
    pub fn product_event_id(&self) -> &EventId {
        &self.product_event_id
    }
    pub const fn observed_revision(&self) -> Revision {
        self.observed_revision
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-contracts")]
pub struct EffectSimulationContext {
    scope: EffectScopeContext,
    relevant_before: RelevantBasisDigest,
    affected_scope: Option<crate::AffectedScopeView>,
}

impl EffectSimulationContext {
    pub(crate) fn new(scope: EffectScopeContext, relevant_before: RelevantBasisDigest) -> Self {
        Self {
            scope,
            relevant_before,
            affected_scope: None,
        }
    }
    pub(crate) fn with_affected_scope(mut self, view: Option<crate::AffectedScopeView>) -> Self {
        self.affected_scope = view;
        self
    }
    pub fn store(&self) -> &StoreIdentity {
        self.scope.store()
    }
    pub fn actor(&self) -> Option<&ActorRef> {
        self.scope.actor()
    }
    pub fn effect_id(&self) -> &EffectId {
        self.scope.effect_id()
    }
    pub fn product_event_id(&self) -> &EventId {
        self.scope.product_event_id()
    }
    pub const fn observed_revision(&self) -> Revision {
        self.scope.observed_revision()
    }
    pub const fn relevant_before(&self) -> RelevantBasisDigest {
        self.relevant_before
    }

    pub fn require_affected_scope(&self) -> Result<&crate::AffectedScopeView, ZapError> {
        self.affected_scope
            .as_ref()
            .ok_or_else(effect_context_error)
    }
}

/// Derives scope and applies the same pure effect kernel to a projected state.
///
/// ```
/// use zap_core::{CommandPayload, EffectContract, EffectScopeContext, StateReader};
/// fn scope<P: CommandPayload>(contract: &dyn EffectContract<P>, state: &dyn StateReader, context: &EffectScopeContext, payload: &P) -> Result<zap_core::EffectScope, zap_wire::ZapError> {
///     contract.scope(state, context, payload)
/// }
/// ```
pub trait EffectContract<P: CommandPayload>: Send + Sync + 'static {
    fn scope(
        &self,
        state: &dyn StateReader,
        context: &EffectScopeContext,
        payload: &P,
    ) -> Result<EffectScope, ZapError>;
    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &P,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

/// Selects immutable prepared bundles from the current typed payload.
///
/// ```
/// use zap_core::{CommandPayload, PayloadEffectBundles, StateReader};
/// fn requests<P: CommandPayload>(adapter: &dyn PayloadEffectBundles<P>, state: &dyn StateReader, payload: &P) -> Result<Vec<zap_core::EffectBundleRequest>, zap_wire::ZapError> {
///     let requests = adapter.requests(state, payload)?;
///     assert!(requests.windows(2).all(|pair| pair[0].request_digest() != pair[1].request_digest()));
///     Ok(requests)
/// }
/// ```
pub trait PayloadEffectBundles<P: CommandPayload>: Send + Sync + 'static {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &P,
    ) -> Result<Vec<EffectBundleRequest>, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-effects")]
pub struct EffectPreflightView {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload_digest: PayloadDigest,
    pub product_event_id: EventId,
    pub predecessors: Vec<EffectId>,
    pub work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub artifacts: Vec<ArtifactDigest>,
    pub reducer_epoch: ReducerEpoch,
    pub observed_revision: Revision,
    pub relevant_before: RelevantBasisDigest,
    pub relevant_after: RelevantBasisDigest,
    pub mutation_digest: EffectMutationDigest,
    pub stable_digest: EffectItemDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-effects")]
pub struct EffectBundlePreflightView {
    pub alternative_id: ChangeAlternativeId,
    pub request_digest: PayloadDigest,
    pub committed_prefix: Vec<EffectId>,
    pub effects: Vec<EffectPreflightView>,
    pub initial_basis: RelevantBasisDigest,
    pub final_basis: RelevantBasisDigest,
    pub digest: EffectPreflightDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-effects")]
pub struct PreparedEffectBundle {
    store: StoreIdentity,
    observed_revision: Revision,
    request: EffectBundleRequest,
    view: EffectBundlePreflightView,
    affected_scopes: Vec<Option<crate::AffectedScopeView>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-effects")]
pub struct PreparedEffectComparison {
    store: StoreIdentity,
    observed_revision: Revision,
    alternatives: Vec<PreparedEffectBundle>,
    basis_request: BasisRequest,
    relevant_basis: RelevantBasisDigest,
}

impl PreparedEffectComparison {
    pub(crate) fn new(
        store: StoreIdentity,
        observed_revision: Revision,
        alternatives: Vec<PreparedEffectBundle>,
        basis_request: BasisRequest,
        relevant_basis: RelevantBasisDigest,
    ) -> Self {
        Self {
            store,
            observed_revision,
            alternatives,
            basis_request,
            relevant_basis,
        }
    }
    pub fn store(&self) -> &StoreIdentity {
        &self.store
    }
    pub const fn observed_revision(&self) -> Revision {
        self.observed_revision
    }
    pub fn alternatives(&self) -> &[PreparedEffectBundle] {
        &self.alternatives
    }
    pub fn basis_request(&self) -> &BasisRequest {
        &self.basis_request
    }
    pub const fn relevant_basis(&self) -> RelevantBasisDigest {
        self.relevant_basis
    }
}

impl PreparedEffectBundle {
    pub(crate) fn new(
        store: StoreIdentity,
        observed_revision: Revision,
        request: EffectBundleRequest,
        view: EffectBundlePreflightView,
        affected_scopes: Vec<Option<crate::AffectedScopeView>>,
    ) -> Self {
        Self {
            store,
            observed_revision,
            request,
            view,
            affected_scopes,
        }
    }
    pub fn store(&self) -> &StoreIdentity {
        &self.store
    }
    pub const fn observed_revision(&self) -> Revision {
        self.observed_revision
    }
    pub fn request(&self) -> &EffectBundleRequest {
        &self.request
    }
    pub fn view(&self) -> &EffectBundlePreflightView {
        &self.view
    }
    pub fn affected_scope(&self, index: usize) -> Option<&crate::AffectedScopeView> {
        self.affected_scopes.get(index).and_then(Option::as_ref)
    }
    pub fn into_parts(self) -> (EffectBundleRequest, EffectBundlePreflightView) {
        (self.request, self.view)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-effects")]
pub struct CommandPreflightRecord {
    pub effect_bundles: Vec<EffectBundlePreflightView>,
    pub affected_scopes: Vec<crate::AffectedScopeView>,
    pub safe_jobs: Vec<crate::SafeJobView>,
    pub packet_resolution: Option<crate::RuntimeJobClaimRecord>,
}

fn has_duplicates<T: Eq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

fn sort_unique<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

fn effect_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE",
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}

fn effect_context_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW",
        "effect simulation requires a transaction-derived affected scope",
        zap_wire::FixSurface::Adapter,
        zap_wire::ErrorDetail::None,
    )
}
