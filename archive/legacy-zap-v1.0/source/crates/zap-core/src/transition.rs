use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use specmark::spec;
use zap_wire::{
    ArtifactDigest, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CommandDigest, CommandEnvelope, CommandHeader, CommandReason, ErrorCode, ErrorDetail,
    EventKind, FixSurface, RouteClass, ZapError,
};

use crate::{
    ActionImpactRequest, AdmittedAuthority, AffectedJobRequest, AffectedJobView,
    AffectedScopeRequest, BasisRequest, CellDescriptor, ChangeSet, CompletionView,
    DispatchEligibilityRequest, DispatchEligibilityView, EffectBundleRequest, EffectContract,
    EffectScope, EffectScopeContext, EffectSimulationContext, PacketResolutionRequest,
    PayloadActionImpact, PayloadArtifacts, PayloadBasisScope, PayloadPacketResolution,
    RuntimeJobClaim, SafeJobRequest, StateReader,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

mod cell;
mod erased;
mod payload;

use cell::DecodedPayload;
pub use cell::{
    ErasedCommandPayload, TransitionCell, ValidatedCommand, ValidatedCommandPreflight,
    ValidatedHeader,
};
use erased::CellAdapter;
pub use erased::ErasedTransitionCell;
pub use payload::{
    CommandPayload, PayloadAffectedJobs, PayloadAffectedScope, PayloadDispatchEligibility,
    PayloadSafeJobs,
};

#[cfg(test)]
#[path = "transition/tests.rs"]
mod tests;

fn registry_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "transition registration, route, payload kind or codec does not match",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn duplicate_cell() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "event kind is registered by more than one transition cell",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn duplicate_adapter() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "one transition cell cannot register the same typed adapter role twice",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transition-registration"
)]
pub struct CellRegistrationBuilder<C: TransitionCell> {
    cell: C,
    basis_scope: Option<Arc<dyn PayloadBasisScope<C::Payload>>>,
    dispatch_scope: Option<Arc<dyn PayloadDispatchEligibility<C::Payload>>>,
    affected_job_scope: Option<Arc<dyn PayloadAffectedJobs<C::Payload>>>,
    artifact_scope: Option<Arc<dyn PayloadArtifacts<C::Payload>>>,
    action_impact_scope: Option<Arc<dyn PayloadActionImpact<C::Payload>>>,
    effect_contract: Option<Arc<dyn EffectContract<C::Payload>>>,
    effect_bundles: Option<Arc<dyn crate::PayloadEffectBundles<C::Payload>>>,
    affected_scope: Option<Arc<dyn PayloadAffectedScope<C::Payload>>>,
    safe_jobs: Option<Arc<dyn PayloadSafeJobs<C::Payload>>>,
    packet_resolution: Option<Arc<dyn PayloadPacketResolution<C::Payload>>>,
}

impl<C: TransitionCell> CellRegistrationBuilder<C> {
    pub fn new(cell: C) -> Self {
        Self {
            cell,
            basis_scope: None,
            dispatch_scope: None,
            affected_job_scope: None,
            artifact_scope: None,
            action_impact_scope: None,
            effect_contract: None,
            effect_bundles: None,
            affected_scope: None,
            safe_jobs: None,
            packet_resolution: None,
        }
    }

    pub fn basis<B>(mut self, adapter: B) -> Result<Self, ZapError>
    where
        B: PayloadBasisScope<C::Payload>,
    {
        if self.basis_scope.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn dispatch<D>(mut self, adapter: D) -> Result<Self, ZapError>
    where
        D: PayloadDispatchEligibility<C::Payload>,
    {
        if self.dispatch_scope.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn affected_jobs<A>(mut self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadAffectedJobs<C::Payload>,
    {
        if self.affected_job_scope.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn artifacts<A>(mut self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadArtifacts<C::Payload>,
    {
        if self.artifact_scope.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn action_impact<I>(mut self, adapter: I) -> Result<Self, ZapError>
    where
        I: PayloadActionImpact<C::Payload>,
    {
        if self
            .action_impact_scope
            .replace(Arc::new(adapter))
            .is_some()
        {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn effect_contract<E>(mut self, adapter: E) -> Result<Self, ZapError>
    where
        E: EffectContract<C::Payload>,
    {
        if self.effect_contract.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn effect_bundles<E>(mut self, adapter: E) -> Result<Self, ZapError>
    where
        E: crate::PayloadEffectBundles<C::Payload>,
    {
        if self.effect_bundles.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn affected_scope<A>(mut self, adapter: A) -> Result<Self, ZapError>
    where
        A: PayloadAffectedScope<C::Payload>,
    {
        if self.affected_scope.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn safe_jobs<J>(mut self, adapter: J) -> Result<Self, ZapError>
    where
        J: PayloadSafeJobs<C::Payload>,
    {
        if self.safe_jobs.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn packet_resolution<R>(mut self, adapter: R) -> Result<Self, ZapError>
    where
        R: PayloadPacketResolution<C::Payload>,
    {
        if self.packet_resolution.replace(Arc::new(adapter)).is_some() {
            return Err(duplicate_adapter());
        }
        Ok(self)
    }

    pub fn build(self) -> Result<CellSet, ZapError> {
        let privileged = matches!(self.cell.descriptor()?.route(), RouteClass::Privileged(_));
        if privileged != self.action_impact_scope.is_some() {
            return Err(registry_invariant());
        }
        CellSet::single_scoped(
            self.cell,
            self.basis_scope,
            self.dispatch_scope,
            self.affected_job_scope,
            self.artifact_scope,
            self.action_impact_scope,
            self.effect_contract,
            self.effect_bundles,
            self.affected_scope,
            self.safe_jobs,
            self.packet_resolution,
        )
    }
}

/// A duplicate-free heterogeneous transition-cell registry.
#[derive(Clone, Default)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transition-registration"
)]
pub struct CellSet {
    cells: BTreeMap<EventKind, Arc<dyn ErasedTransitionCell>>,
}

impl CellSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn single<C: TransitionCell>(cell: C) -> Result<Self, ZapError> {
        CellRegistrationBuilder::new(cell).build()
    }

    pub fn single_with_basis<C, B>(cell: C, basis: B) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        B: PayloadBasisScope<C::Payload>,
    {
        CellRegistrationBuilder::new(cell).basis(basis)?.build()
    }

    pub fn single_with_dispatch<C, D>(cell: C, dispatch: D) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        D: PayloadDispatchEligibility<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .dispatch(dispatch)?
            .build()
    }

    pub fn single_with_scopes<C, B, D>(cell: C, basis: B, dispatch: D) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        B: PayloadBasisScope<C::Payload>,
        D: PayloadDispatchEligibility<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .basis(basis)?
            .dispatch(dispatch)?
            .build()
    }

    pub fn single_with_affected_jobs<C, A>(cell: C, affected: A) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        A: PayloadAffectedJobs<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .affected_jobs(affected)?
            .build()
    }

    pub fn single_with_basis_and_affected_jobs<C, B, A>(
        cell: C,
        basis: B,
        affected: A,
    ) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        B: PayloadBasisScope<C::Payload>,
        A: PayloadAffectedJobs<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .basis(basis)?
            .affected_jobs(affected)?
            .build()
    }

    pub fn single_with_artifacts<C, A>(cell: C, artifacts: A) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        A: PayloadArtifacts<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .artifacts(artifacts)?
            .build()
    }

    pub fn single_with_basis_and_artifacts<C, B, A>(
        cell: C,
        basis: B,
        artifacts: A,
    ) -> Result<Self, ZapError>
    where
        C: TransitionCell,
        B: PayloadBasisScope<C::Payload>,
        A: PayloadArtifacts<C::Payload>,
    {
        CellRegistrationBuilder::new(cell)
            .basis(basis)?
            .artifacts(artifacts)?
            .build()
    }

    #[allow(clippy::too_many_arguments)]
    fn single_scoped<C: TransitionCell>(
        cell: C,
        basis_scope: Option<Arc<dyn PayloadBasisScope<C::Payload>>>,
        dispatch_scope: Option<Arc<dyn PayloadDispatchEligibility<C::Payload>>>,
        affected_job_scope: Option<Arc<dyn PayloadAffectedJobs<C::Payload>>>,
        artifact_scope: Option<Arc<dyn PayloadArtifacts<C::Payload>>>,
        action_impact_scope: Option<Arc<dyn PayloadActionImpact<C::Payload>>>,
        effect_contract: Option<Arc<dyn EffectContract<C::Payload>>>,
        effect_bundles: Option<Arc<dyn crate::PayloadEffectBundles<C::Payload>>>,
        affected_scope: Option<Arc<dyn PayloadAffectedScope<C::Payload>>>,
        safe_jobs: Option<Arc<dyn PayloadSafeJobs<C::Payload>>>,
        packet_resolution: Option<Arc<dyn PayloadPacketResolution<C::Payload>>>,
    ) -> Result<Self, ZapError> {
        let descriptor = if packet_resolution.is_some() {
            cell.descriptor()?.requiring_packet_resolution()
        } else {
            cell.descriptor()?
        };
        let declared_kind = EventKind::parse(C::Payload::KIND)?;
        if descriptor.kind() != &declared_kind {
            return Err(registry_invariant());
        }
        let mut cells: BTreeMap<EventKind, Arc<dyn ErasedTransitionCell>> = BTreeMap::new();
        cells.insert(
            declared_kind,
            Arc::new(CellAdapter {
                cell,
                descriptor,
                basis_scope,
                dispatch_scope,
                affected_job_scope,
                artifact_scope,
                action_impact_scope,
                effect_contract,
                effect_bundles,
                affected_scope,
                safe_jobs,
                packet_resolution,
            }),
        );
        Ok(Self { cells })
    }

    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for (kind, cell) in set.cells {
                if result.cells.insert(kind, cell).is_some() {
                    return Err(duplicate_cell());
                }
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn kinds(&self) -> impl Iterator<Item = &EventKind> {
        self.cells.keys()
    }

    pub fn has_effect_contract(&self, kind: &EventKind) -> bool {
        self.cells
            .get(kind)
            .is_some_and(|cell| cell.has_effect_contract())
    }

    pub fn descriptor(&self, kind: &EventKind) -> Option<&CellDescriptor> {
        self.cells.get(kind).map(|cell| cell.descriptor())
    }

    pub(crate) fn cell(&self, kind: &EventKind) -> Option<&dyn ErasedTransitionCell> {
        self.cells.get(kind).map(Arc::as_ref)
    }

    pub(crate) fn has_privileged_routes(&self) -> bool {
        self.cells
            .values()
            .any(|cell| matches!(cell.descriptor().route(), RouteClass::Privileged(_)))
    }

    pub(crate) fn has_effect_bundles(&self) -> bool {
        self.cells.values().any(|cell| cell.has_effect_bundles())
    }

    pub(crate) fn has_affected_scope(&self) -> bool {
        self.cells.values().any(|cell| cell.has_affected_scope())
    }

    pub(crate) fn has_safe_jobs(&self) -> bool {
        self.cells.values().any(|cell| cell.has_safe_jobs())
    }

    pub(crate) fn requires_packet_resolution(&self) -> bool {
        self.cells
            .values()
            .any(|cell| cell.descriptor().requires_packet_resolution())
    }
}

/// A total route classification map for registered event kinds.
#[derive(Clone, Default)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transition-registration"
)]
pub struct RouteRegistry {
    routes: BTreeMap<EventKind, RouteClass>,
}

impl RouteRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn single(kind: EventKind, route: RouteClass) -> Self {
        Self {
            routes: BTreeMap::from([(kind, route)]),
        }
    }

    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for (kind, route) in set.routes {
                if result.routes.insert(kind, route).is_some() {
                    return Err(ZapError::from_static(
                        ErrorCode::DuplicateIdentity,
                        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                        "event kind has more than one route classification",
                        FixSurface::Configuration,
                        ErrorDetail::None,
                    ));
                }
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn route(&self, kind: &EventKind) -> Option<&RouteClass> {
        self.routes.get(kind)
    }

    pub fn validate_cells(&self, cells: &CellSet) -> Result<(), ZapError> {
        let route_keys = self.routes.keys().collect::<Vec<_>>();
        let cell_keys = cells.kinds().collect::<Vec<_>>();
        if route_keys != cell_keys {
            return Err(registry_invariant());
        }
        for (kind, route) in &self.routes {
            if cells
                .descriptor(kind)
                .is_none_or(|descriptor| descriptor.route() != route)
            {
                return Err(registry_invariant());
            }
        }
        Ok(())
    }
}
