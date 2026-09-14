use serde::Serialize;
use specmark::spec;
use zap_core::{StateReader, StateReaderExt};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, PayloadDigest, WorkId, ZapError};

use super::{MapAssessmentFreshness, MapWorkAssessmentRecord};
use crate::admission_indexes::{AdmissionIndexBudget, selected_active_contract_for_work};
use crate::control::{TaskContractRecord, WorkRecord};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-BASIS");

const BASIS_ALGORITHM: &str = "zap.map.work-assessment-basis/exact-work-active-contracts/v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkAssessmentSource {
    pub(crate) work: WorkRecord,
    pub(crate) active_contract: Option<TaskContractRecord>,
}

#[derive(Serialize)]
struct WorkAssessmentBasis<'a> {
    algorithm: &'static str,
    work: &'a WorkRecord,
    active_contract: &'a Option<TaskContractRecord>,
}

pub(crate) fn load_work_assessment_source(
    state: &dyn StateReader,
    work_id: &WorkId,
    budget: &mut AdmissionIndexBudget,
) -> Result<Option<WorkAssessmentSource>, ZapError> {
    let Some(work) = state.get_typed::<WorkRecord>(work_id)? else {
        return Ok(None);
    };
    let active_contract = selected_active_contract_for_work(state, work_id, budget)?;
    Ok(Some(WorkAssessmentSource {
        work,
        active_contract,
    }))
}

pub(crate) fn work_assessment_basis_from_source(
    source: &WorkAssessmentSource,
) -> Result<PayloadDigest, ZapError> {
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &WorkAssessmentBasis {
            algorithm: BASIS_ALGORITHM,
            work: &source.work,
            active_contract: &source.active_contract,
        },
    )?;
    Ok(PayloadDigest::hash(bytes.as_bytes()))
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-BASIS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub fn work_assessment_basis(
    state: &dyn StateReader,
    work_id: &WorkId,
) -> Result<PayloadDigest, ZapError> {
    let mut budget = AdmissionIndexBudget::default();
    let source = load_work_assessment_source(state, work_id, &mut budget)?
        .ok_or_else(super::missing_work)?;
    work_assessment_basis_from_source(&source)
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-BASIS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub fn work_assessment_freshness(
    state: &dyn StateReader,
    assessment: &MapWorkAssessmentRecord,
) -> Result<MapAssessmentFreshness, ZapError> {
    assessment.validate()?;
    match work_assessment_basis(state, &assessment.work_id) {
        Ok(current) if current == assessment.source_fingerprint => {
            Ok(MapAssessmentFreshness::Current)
        }
        Ok(_) => Ok(MapAssessmentFreshness::Stale),
        Err(error)
            if matches!(
                error.code,
                ErrorCode::MissingReference | ErrorCode::UnsupportedEpoch | ErrorCode::Unavailable
            ) =>
        {
            Ok(MapAssessmentFreshness::Unavailable)
        }
        Err(error) => Err(error),
    }
}
