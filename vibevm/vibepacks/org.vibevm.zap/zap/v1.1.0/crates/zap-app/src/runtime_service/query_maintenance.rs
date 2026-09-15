use zap_api::{
    AffectedTraversalBeginRequest, AffectedTraversalCancelRequest,
    AffectedTraversalContinueRequest, AffectedTraversalView, IndexRebuildRequest, IndexRebuildView,
};
use zap_core::{CredentialAuthority, PrincipalRole, SecretInput};
use zap_store::{TraversalAdvanceRequest, TraversalAdvanceResult, TraversalSessionSpec};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, PayloadDigest, ZapError};

use super::application::{ApplicationService, service_error};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

pub(crate) fn package_index_families() -> Result<Vec<zap_core::IndexFamily>, ZapError> {
    let mut families = zap_domain::viewer_graph_index_families()?;
    families.extend(zap_core::affected_job_index_families()?);
    families.extend(zap_runtime::runtime_index_families()?);
    families.sort();
    families.dedup();
    Ok(families)
}

pub(crate) fn package_index_algorithms() -> Result<Vec<zap_core::IndexAlgorithm>, ZapError> {
    let mut algorithms = zap_domain::viewer_index_algorithms()?;
    algorithms.extend(zap_core::affected_job_index_algorithms()?);
    algorithms.extend(zap_runtime::runtime_index_algorithms()?);
    algorithms.sort();
    Ok(algorithms)
}

impl ApplicationService {
    pub fn rebuild_viewer_indexes(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &IndexRebuildRequest,
    ) -> Result<IndexRebuildView, ZapError> {
        self.authenticate_traversal_owner(credential_id, secret)?;
        if request.store != *self.identity() {
            return Err(service_error(
                ErrorCode::Unauthorized,
                "index rebuild requires the configured Owner and exact store identity",
            ));
        }
        let receipt = self.store.rebuild_indexes_v2(
            package_index_families()?,
            package_index_algorithms()?,
            request.expected_revision,
        )?;
        Ok(IndexRebuildView {
            catalog: receipt.catalog,
            row_count: receipt.row_count,
        })
    }

    pub fn begin_affected_traversal(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &AffectedTraversalBeginRequest,
    ) -> Result<AffectedTraversalView, ZapError> {
        let principal = self.authenticate_traversal_owner(credential_id, secret)?;
        self.validate_traversal_budgets(
            request.node_budget,
            request.edge_budget,
            request.maximum_state_nodes,
        )?;
        if request.store != *self.identity() || request.expected_revision != self.store.head()? {
            return Err(service_error(
                ErrorCode::StaleRevision,
                "affected traversal begin does not match the current store revision",
            ));
        }
        let focus = request.focus.canonical()?;
        let _: zap_domain::viewer_queries::ViewerNodeId = focus.decode_json()?;
        let catalog = self.store.index_catalog()?.ok_or_else(|| {
            service_error(
                ErrorCode::Unavailable,
                "affected traversal requires a current derived index catalog",
            )
        })?;
        let maximum_allowed_state_nodes = u64::from(self.config.runtime.page_limit)
            .saturating_mul(u64::from(self.config.runtime.page_limit));
        let maximum_cached_response_bytes = u32::try_from(
            self.config
                .material_adapters
                .maximum_material_bytes
                .min(u64::from(u32::MAX)),
        )
        .map_err(|_| service_error(ErrorCode::LimitExceeded, "response budget is too large"))?;
        self.store.begin_traversal_session(TraversalSessionSpec {
            session_id: request.session_id.clone(),
            principal_id: principal.principal_id().clone(),
            store: request.store.clone(),
            revision: request.expected_revision,
            query_epoch: catalog.query_epoch,
            catalog_version: catalog.version,
            focus: focus.as_bytes().to_vec(),
            initial_maximum_state_nodes: request.maximum_state_nodes,
            maximum_allowed_state_nodes,
            maximum_cached_response_bytes,
        })?;
        let digest = PayloadDigest::hash(
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, request)?.as_bytes(),
        );
        self.advance_affected_traversal(
            principal.principal_id(),
            &request.session_id,
            request.expected_revision,
            0,
            request.node_budget,
            request.edge_budget,
            request.maximum_state_nodes,
            digest,
        )
    }

    pub fn continue_affected_traversal(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &AffectedTraversalContinueRequest,
    ) -> Result<AffectedTraversalView, ZapError> {
        let principal = self.authenticate_traversal_owner(credential_id, secret)?;
        self.validate_traversal_budgets(
            request.node_budget,
            request.edge_budget,
            request.maximum_state_nodes,
        )?;
        let digest = PayloadDigest::hash(
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, request)?.as_bytes(),
        );
        self.advance_affected_traversal(
            principal.principal_id(),
            &request.session_id,
            self.store.head()?,
            request.expected_generation,
            request.node_budget,
            request.edge_budget,
            request.maximum_state_nodes,
            digest,
        )
    }

    pub fn cancel_affected_traversal(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &AffectedTraversalCancelRequest,
    ) -> Result<AffectedTraversalView, ZapError> {
        let principal = self.authenticate_traversal_owner(credential_id, secret)?;
        let receipt = self
            .store
            .cancel_traversal_session(&request.session_id, principal.principal_id())?;
        Ok(AffectedTraversalView {
            session_id: receipt.session_id,
            store: self.identity().clone(),
            revision: self.store.head()?,
            generation: receipt.generation,
            progress: zap_core::DerivedTraversalProgress {
                visited: 0,
                frontier: 0,
                generation: receipt.generation,
                maximum_state_nodes: 0,
            },
            items: Vec::new(),
            complete: false,
            exact_retry: receipt.already_canceled,
            quota_required_state_nodes: None,
            repair: None,
            canceled: true,
            cleanup_complete: Some(receipt.cleanup_complete),
            cleanup_removed_rows: receipt.removed_rows,
            algorithm: "durable_affected_session_v1".to_owned(),
        })
    }

    fn authenticate_traversal_owner(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
    ) -> Result<zap_core::AuthenticatedPrincipal, ZapError> {
        let principal = self.service.credential_authority().authenticate(
            credential_id,
            SecretInput::new(secret),
            &self.identity().campaign_id,
        )?;
        if principal.role() != PrincipalRole::Owner {
            return Err(service_error(
                ErrorCode::Unauthorized,
                "affected traversal maintenance requires the configured Owner",
            ));
        }
        Ok(principal)
    }

    fn validate_traversal_budgets(
        &self,
        node_budget: u32,
        edge_budget: u32,
        maximum_state_nodes: u64,
    ) -> Result<(), ZapError> {
        let maximum_page = self.config.runtime.page_limit;
        let maximum_state = u64::from(maximum_page).saturating_mul(u64::from(maximum_page));
        if node_budget == 0
            || edge_budget < 8
            || node_budget > maximum_page
            || edge_budget > maximum_page
            || maximum_state_nodes == 0
            || maximum_state_nodes > maximum_state
        {
            return Err(service_error(
                ErrorCode::LimitExceeded,
                "affected traversal budgets exceed the configured runtime allowance",
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_affected_traversal(
        &self,
        principal_id: &zap_wire::PrincipalId,
        session_id: &zap_wire::OperationId,
        revision: zap_wire::Revision,
        expected_generation: u64,
        node_budget: u32,
        edge_budget: u32,
        maximum_state_nodes: u64,
        request_digest: PayloadDigest,
    ) -> Result<AffectedTraversalView, ZapError> {
        let result = self.store.advance_traversal_session(
            &TraversalAdvanceRequest {
                session_id: session_id.clone(),
                principal_id: principal_id.clone(),
                expected_generation,
                request_digest,
                maximum_state_nodes,
            },
            |state| {
                let step = zap_domain::viewer_queries::advance_affected_traversal(
                    state,
                    node_budget,
                    edge_budget,
                )?;
                zap_domain::viewer_queries::encode_affected_step(&step)
            },
        )?;
        let (generation, progress, bytes, exact_retry) = match result {
            TraversalAdvanceResult::Advanced {
                generation,
                progress,
                response,
            } => (generation, progress, response, false),
            TraversalAdvanceResult::ExactRetry {
                generation,
                progress,
                response,
            } => (generation, progress, response, true),
        };
        let step = zap_domain::viewer_queries::decode_affected_step(&bytes)?;
        let items = step
            .nodes
            .iter()
            .map(|node| {
                Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, node)?
                    .as_bytes()
                    .to_vec())
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        Ok(AffectedTraversalView {
            session_id: session_id.clone(),
            store: self.identity().clone(),
            revision,
            generation,
            progress,
            items,
            complete: step.complete,
            exact_retry,
            quota_required_state_nodes: step.quota.as_ref().map(|quota| quota.required_state_nodes),
            repair: step.quota.map(|quota| quota.repair),
            canceled: false,
            cleanup_complete: None,
            cleanup_removed_rows: 0,
            algorithm: step.algorithm,
        })
    }
}
