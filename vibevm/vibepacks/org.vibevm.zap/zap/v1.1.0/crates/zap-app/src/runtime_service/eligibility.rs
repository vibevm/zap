specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, DispatchEligibilityBlocker, DispatchEligibilityProvider,
    DispatchEligibilityRequest, DispatchEligibilityView, StateReader, StateReaderExt,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::seams::WorkState;
use zap_runtime::RuntimeJobRecord;
use zap_wire::SubjectRef;

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#runtime-composition")]
pub struct ApplicationDispatchEligibilityProvider;

impl DispatchEligibilityProvider for ApplicationDispatchEligibilityProvider {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &DispatchEligibilityRequest,
    ) -> Result<DispatchEligibilityView, zap_wire::ZapError> {
        let input = &request.input;
        let work = state.get_typed::<WorkRecord>(&input.work_id)?;
        let contract = state.get_typed::<TaskContractRecord>(&input.contract_id)?;
        let job = work
            .as_ref()
            .and_then(|work| work.active_job.as_ref())
            .map(|job_id| state.get_typed::<RuntimeJobRecord>(job_id))
            .transpose()?
            .flatten();
        let mut blockers = Vec::new();
        let contract_matches = contract.is_some_and(|contract| {
            contract.active
                && contract.work_id == input.work_id
                && contract.version.get() == input.contract_version.get()
                && contract.contract_digest == input.contract_digest
        });
        let job_matches = job.is_some_and(|job| {
            job.work_id == input.work_id
                && job.contract_id == input.contract_id
                && job.contract_version == input.contract_version
                && job.contract_digest == input.contract_digest
                && job.validation_generation == input.validation_generation
                && job.relevant_basis == input.relevant_basis
                && job.read_subjects == input.read_subjects
                && job.write_subjects == input.write_subjects
                && job.resources == input.resources
                && job.integration_owner == input.integration_owner
                && job.delivery_route == input.delivery_route
        });
        if work.as_ref().is_none_or(|work| {
            work.state != WorkState::Active
                || work.validation_generation != input.validation_generation.get()
        }) || !contract_matches
            || !job_matches
        {
            blockers.push(DispatchEligibilityBlocker::ContractChanged);
        }
        let basis = BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Dispatch(input.work_id.clone()),
            roots: vec![SubjectRef::Work(input.work_id.clone())],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?;
        if DomainBasisProvider.relevant_basis(state, &basis)?.digest != input.relevant_basis {
            blockers.push(DispatchEligibilityBlocker::BasisChanged);
        }
        Ok(DispatchEligibilityView::new(
            request.digest,
            state.revision(),
            blockers,
        ))
    }
}
