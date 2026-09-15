use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{StateReader, StateReaderExt};
use zap_wire::{
    ErrorCode, EvidenceId, InformationSelectionId, PayloadDigest, SourceId, WorkId, ZapError,
};

use crate::control::WorkRecord;
use crate::seams::SourceCapture;

use super::{
    InformationOpportunityRecord, InformationSelectionRecord, InformationStopEnforcement,
    InformationStopRule, allowed_work_type, information_opportunity_basis,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE");

#[derive(Clone, Copy)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#execution")]
pub struct InformationLoweringBinding<'a> {
    pub selection_id: &'a InformationSelectionId,
    pub work: &'a WorkRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#execution")]
pub struct InformationExecutionContext {
    pub selection_id: InformationSelectionId,
    pub selection_revision: zap_wire::Revision,
    pub work_id: WorkId,
    pub work_type: crate::seams::WorkType,
    pub opportunity_fingerprint: PayloadDigest,
    pub basis_fingerprint: PayloadDigest,
    pub observation_sought: zap_wire::BoundedText<4096>,
    pub sources: Vec<SourceCapture>,
    pub source_ids: Vec<SourceId>,
    pub stop_rule: InformationStopRule,
    pub stop_rule_fingerprint: PayloadDigest,
    pub required_safe_stop_boundary: zap_wire::BoundedText<4096>,
    pub satisfying_evidence_ids: Vec<EvidenceId>,
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#execution")]
pub fn selected_information_execution_context(
    state: &dyn StateReader,
    selection_id: &InformationSelectionId,
) -> Result<InformationExecutionContext, ZapError> {
    let selection = load_current_selection(state, selection_id)?;
    let opportunity = state
        .get_typed::<InformationOpportunityRecord>(&selection.opportunity_id)?
        .ok_or_else(|| {
            reuse_error(
                ErrorCode::MissingReference,
                "selected opportunity is missing",
            )
        })?;
    validate_current_pair(state, &selection, &opportunity)?;
    let stop_rule_fingerprint = digest(&opportunity.content.stop_rule)?;
    let required_safe_stop_boundary =
        safe_stop_boundary(&opportunity.content.stop_rule, stop_rule_fingerprint)?;
    let source_ids = opportunity.content.source_ids();
    Ok(InformationExecutionContext {
        selection_id: selection.selection_id,
        selection_revision: selection.revision,
        work_id: selection.candidate_work_id,
        work_type: selection.work_type,
        opportunity_fingerprint: opportunity.semantic_fingerprint,
        basis_fingerprint: opportunity.basis_fingerprint,
        observation_sought: opportunity.content.observation_sought.clone(),
        source_ids,
        sources: opportunity
            .content
            .sources
            .iter()
            .map(|row| row.capture.clone())
            .collect(),
        stop_rule: opportunity.content.stop_rule,
        stop_rule_fingerprint,
        required_safe_stop_boundary,
        satisfying_evidence_ids: opportunity.content.satisfying_evidence_ids,
    })
}

fn safe_stop_boundary(
    rule: &InformationStopRule,
    fingerprint: PayloadDigest,
) -> Result<zap_wire::BoundedText<4096>, ZapError> {
    let attempts = rule
        .maximum_attempts
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    let agent_hours = rule
        .maximum_agent_hours
        .map_or_else(|| "unknown".to_owned(), |value| value.get().to_string());
    let elapsed = rule
        .maximum_elapsed
        .map_or_else(|| "unknown".to_owned(), |value| value.get().to_string());
    zap_wire::BoundedText::parse(&format!(
        "information-stop {}: observed={}; attempts={}; agent_micro_hours={}; elapsed_micro_hours={}; source_drift={}; enforcement={:?}; {}",
        fingerprint,
        rule.stop_when_observed,
        attempts,
        agent_hours,
        elapsed,
        rule.stop_on_source_drift,
        rule.enforcement,
        rule.explanation.as_str(),
    ))
}

fn digest<T: Serialize>(value: &T) -> Result<PayloadDigest, ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, value)?.digest())
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#execution")]
pub(crate) fn validate_lowering_information_bindings(
    state: &dyn StateReader,
    bindings: &[InformationLoweringBinding<'_>],
) -> Result<(), ZapError> {
    if bindings.is_empty() {
        return Ok(());
    }
    let mut selection_ids = BTreeSet::new();
    let mut work_ids = BTreeSet::new();
    for binding in bindings {
        if !selection_ids.insert(binding.selection_id.clone())
            || !work_ids.insert(binding.work.work_id.clone())
        {
            return Err(reuse_error(
                ErrorCode::DuplicateIdentity,
                "one lowering cannot duplicate an information selection or candidate Work",
            ));
        }
        let selection = state
            .get_typed::<InformationSelectionRecord>(binding.selection_id)?
            .ok_or_else(|| {
                reuse_error(
                    ErrorCode::MissingReference,
                    "information selection is missing",
                )
            })?;
        let opportunity = state
            .get_typed::<InformationOpportunityRecord>(&selection.opportunity_id)?
            .ok_or_else(|| {
                reuse_error(
                    ErrorCode::MissingReference,
                    "selected information opportunity is missing",
                )
            })?;
        validate_current_pair(state, &selection, &opportunity)?;
        if selection.candidate_work_id != binding.work.work_id
            || selection.work_type != binding.work.work_type
            || !allowed_work_type(binding.work.work_type)
        {
            return Err(reuse_error(
                ErrorCode::Conflict,
                "lowered research must use the exact selected Work identity and Evidence or Decision type",
            ));
        }
        if let Some(existing) = state.get_typed::<WorkRecord>(&selection.candidate_work_id)?
            && existing.work_type != binding.work.work_type
        {
            return Err(reuse_error(
                ErrorCode::Conflict,
                "information retry changed the stable Work type",
            ));
        }
        if opportunity.content.stop_rule.enforcement == InformationStopEnforcement::ContractBoundary
            && opportunity
                .content
                .stop_rule
                .explanation
                .as_str()
                .trim()
                .is_empty()
        {
            return Err(reuse_error(
                ErrorCode::InvalidValue,
                "contract-bound information stop rule lost its safe-boundary text",
            ));
        }
    }
    Ok(())
}

fn load_current_selection(
    state: &dyn StateReader,
    selection_id: &InformationSelectionId,
) -> Result<InformationSelectionRecord, ZapError> {
    state
        .get_typed::<InformationSelectionRecord>(selection_id)?
        .ok_or_else(|| {
            reuse_error(
                ErrorCode::MissingReference,
                "information selection is missing",
            )
        })
}

fn validate_current_pair(
    state: &dyn StateReader,
    selection: &InformationSelectionRecord,
    opportunity: &InformationOpportunityRecord,
) -> Result<(), ZapError> {
    if selection.opportunity_revision != opportunity.revision
        || selection.opportunity_fingerprint != opportunity.semantic_fingerprint
        || selection.basis_fingerprint != opportunity.basis_fingerprint
        || information_opportunity_basis(state, &opportunity.content)?
            != opportunity.basis_fingerprint
    {
        return Err(reuse_error(
            ErrorCode::StaleRevision,
            "information selection is stale against its decision or source basis",
        ));
    }
    Ok(())
}

fn reuse_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        super::REUSE_REQUIREMENT,
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zap_core::{
        EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, PageLimit, RecordFamily,
        StateReader, StoreIdentity,
    };
    use zap_wire::{
        BaseId, CampaignId, CodecEpoch, ReducerEpoch, Revision, StoreEpoch, StoreId, ZapError,
    };

    struct NoReadState {
        identity: StoreIdentity,
    }

    impl StateReader for NoReadState {
        fn identity(&self) -> StoreIdentity {
            self.identity.clone()
        }

        fn revision(&self) -> Revision {
            Revision::GENESIS
        }

        fn get_erased(
            &self,
            _family: &RecordFamily,
            _key: &EncodedRecordKey,
        ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
            panic!("empty information bindings must not read records")
        }

        fn scan_erased(
            &self,
            _family: &RecordFamily,
            _range: EncodedKeyRange,
            _limit: PageLimit,
        ) -> Result<ErasedRecordPage, ZapError> {
            panic!("empty information bindings must not scan unrelated opportunities")
        }
    }

    #[test]
    fn empty_information_bindings_do_no_store_work() -> Result<(), ZapError> {
        let state = NoReadState {
            identity: StoreIdentity {
                store_id: StoreId::parse("store.information.empty")?,
                campaign_id: CampaignId::parse("campaign.information.empty")?,
                base_id: BaseId::parse("base.information.empty")?,
                store_epoch: StoreEpoch::ZAP2,
                codec_epoch: CodecEpoch::CURRENT,
                reducer_epoch: ReducerEpoch::new(1)?,
            },
        };
        super::validate_lowering_information_bindings(&state, &[])
    }
}
