fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#ACTUAL-RUNNER",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn eligibility_request(work: &WorkExecutionView) -> Result<DispatchEligibilityRequest, ZapError> {
    DispatchEligibilityRequest::build(DispatchEligibilityRequestInput {
        work_id: work.work_id.clone(),
        contract_id: work.contract_id.clone(),
        contract_version: work.contract_version,
        contract_digest: work.contract_digest,
        validation_generation: work.validation_generation,
        relevant_basis: work.relevant_basis,
        read_subjects: work.read_subjects.clone(),
        write_subjects: work.write_subjects.clone(),
        resources: work.resources.clone(),
        integration_owner: work.integration_owner.clone(),
        delivery_route: work.delivery_route.clone(),
    })
}

include!("../../../../zap-runtime/tests/runtime_persistence/runtime_support.rs");

struct SnapshotPort {
    store: RedbStore,
}

impl CampaignReadPort for SnapshotPort {
    fn snapshot(&self, at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError> {
        Ok(Box::new(TransactionStore::read(&self.store, at)?))
    }

    fn frontier(&self, _request: FrontierRequest) -> Result<Page<FrontierWorkView>, ZapError> {
        Err(test_error(
            "candidate fixture does not schedule frontier work",
        ))
    }

    fn work_execution_view(
        &self,
        _work: &WorkId,
        _at: ReadAt,
    ) -> Result<WorkExecutionView, ZapError> {
        Err(test_error(
            "candidate fixture consumes the already sealed job view",
        ))
    }

    fn current_packet(
        &self,
        _work: &WorkId,
        _at: ReadAt,
    ) -> Result<Option<CurrentPacketSelection>, ZapError> {
        Err(test_error(
            "candidate fixture consumes the already sealed packet",
        ))
    }

    fn explain_readiness(&self, _work: &WorkId, _at: ReadAt) -> Result<ReadinessView, ZapError> {
        Err(test_error("candidate fixture does not reschedule work"))
    }

    fn completion_view(&self, _at: ReadAt) -> Result<CompletionView, ZapError> {
        Err(test_error(
            "candidate fixture does not evaluate campaign completion",
        ))
    }
}
