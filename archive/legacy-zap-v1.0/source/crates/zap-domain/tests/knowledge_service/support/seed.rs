#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SeedState {
    pub(super) sources: Vec<SourceRecord>,
    pub(super) evidence: Vec<EvidenceAdjudicationRecord>,
    pub(super) dependencies: Vec<KnowledgeDependencyRecord>,
    pub(super) dependency_replacements: Vec<KnowledgeDependencyRecord>,
    pub(super) dependency_removals: Vec<KnowledgeDependencyRecord>,
    pub(super) closures: Vec<KnowledgeClosureRecord>,
    pub(super) outcomes: Vec<OutcomeRecord>,
    pub(super) intents: Vec<IntentRecord>,
    pub(super) charters: Vec<CharterRecord>,
    pub(super) work: Vec<WorkRecord>,
    pub(super) work_replacements: Vec<WorkRecord>,
    pub(super) work_removals: Vec<WorkRecord>,
    pub(super) contracts: Vec<TaskContractRecord>,
    pub(super) obligations: Vec<ObligationRecord>,
    pub(super) applicability: Vec<SourceApplicabilityRecord>,
    pub(super) facts: Vec<FactRecord>,
    pub(super) regions: Vec<RegionRecord>,
    pub(super) semantic_assessments: Vec<SemanticAssessmentRecord>,
    pub(super) reviews: Vec<AdaptiveReviewRecord>,
    pub(super) review_replacements: Vec<AdaptiveReviewRecord>,
    pub(super) candidates: Vec<CandidateProvenanceInput>,
    pub(super) candidate_reviews: Vec<CandidateReviewRecord>,
    pub(super) jobs: Vec<WorkExecutionObservationRecord>,
    pub(super) job_replacements: Vec<WorkExecutionObservationRecord>,
    pub(super) releases: Vec<WorkRevalidationReleaseInput>,
}

impl CanonicalEncode for SeedState {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for SeedState {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for SeedState {
    const KIND: &'static str = SEED_KIND;
}

pub(super) struct SeedStateCell;

impl TransitionCell for SeedStateCell {
    type Payload = SeedState;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(SourceRecord::FAMILY)?,
            RecordFamily::parse(EvidenceAdjudicationRecord::FAMILY)?,
            RecordFamily::parse(KnowledgeDependencyRecord::FAMILY)?,
            RecordFamily::parse(KnowledgeClosureRecord::FAMILY)?,
            RecordFamily::parse(OutcomeRecord::FAMILY)?,
            RecordFamily::parse(IntentRecord::FAMILY)?,
            RecordFamily::parse(CharterRecord::FAMILY)?,
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(TaskContractRecord::FAMILY)?,
            RecordFamily::parse(ObligationRecord::FAMILY)?,
            RecordFamily::parse(SourceApplicabilityRecord::FAMILY)?,
            RecordFamily::parse(FactRecord::FAMILY)?,
            RecordFamily::parse(RegionRecord::FAMILY)?,
            RecordFamily::parse(SemanticAssessmentRecord::FAMILY)?,
            RecordFamily::parse(AdaptiveReviewRecord::FAMILY)?,
            RecordFamily::parse(CandidateProvenanceRecord::FAMILY)?,
            RecordFamily::parse(CandidateReviewRecord::FAMILY)?,
            RecordFamily::parse(WorkExecutionObservationRecord::FAMILY)?,
            RecordFamily::parse(WorkRevalidationReleaseRecord::FAMILY)?,
        ];
        affected_records.sort();
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(SEED_KIND)?,
            route: RouteClass::OwnerControl(ControlClass::CharterActivate),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes: test_index_families()?,
            requirements: vec![RequirementRef::parse(TEST_REQ)?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        for row in &command.payload().sources {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().evidence {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().dependencies {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().dependency_replacements {
            let current = state
                .get_typed::<KnowledgeDependencyRecord>(&row.edge_id)?
                .ok_or_else(test_error)?;
            changes.replace(current.revision, row.clone())?;
        }
        for row in &command.payload().dependency_removals {
            let current = state
                .get_typed::<KnowledgeDependencyRecord>(&row.edge_id)?
                .ok_or_else(test_error)?;
            changes.remove::<KnowledgeDependencyRecord>(current.edge_id, current.revision)?;
        }
        for row in &command.payload().closures {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().outcomes {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().intents {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().charters {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().work {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().work_replacements {
            let current = state
                .get_typed::<WorkRecord>(&row.work_id)?
                .ok_or_else(test_error)?;
            changes.replace(current.revision, row.clone())?;
        }
        for row in &command.payload().work_removals {
            let current = state
                .get_typed::<WorkRecord>(&row.work_id)?
                .ok_or_else(test_error)?;
            changes.remove::<WorkRecord>(current.work_id, current.revision)?;
        }
        for row in &command.payload().contracts {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().obligations {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().applicability {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().facts {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().regions {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().semantic_assessments {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().reviews {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().review_replacements {
            let current = state
                .get_typed::<AdaptiveReviewRecord>(&row.review_id)?
                .ok_or_else(test_error)?;
            changes.replace(current.revision, row.clone())?;
        }
        for row in &command.payload().candidates {
            changes.insert(CandidateProvenanceRecord::new(row.clone())?)?;
        }
        for row in &command.payload().candidate_reviews {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().jobs {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().job_replacements {
            let current = state
                .get_typed::<WorkExecutionObservationRecord>(&row.job_id)?
                .ok_or_else(test_error)?;
            changes.replace(current.revision, row.clone())?;
        }
        for row in &command.payload().releases {
            changes.insert(WorkRevalidationReleaseRecord::new(row.clone())?)?;
        }
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}
