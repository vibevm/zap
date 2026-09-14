#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedPayload {
    charters: Vec<CharterRecord>,
    intents: Vec<IntentRecord>,
    outcomes: Vec<OutcomeRecord>,
    baselines: Vec<ChangeBaselineRecord>,
    policies: Vec<ChangePolicyRecord>,
    assessments: Vec<ChangeAssessmentRecord>,
    admissions: Vec<ChangeAdmissionRecord>,
    holds: Vec<ChangeHoldRecord>,
    decisions: Vec<OwnerChangeDecisionRecord>,
    pauses: Vec<PauseRecord>,
    exceptions: Vec<ActionExceptionRecord>,
    work: Vec<WorkRecord>,
    work_replacements: Vec<WorkRecord>,
    reviews: Vec<AdaptiveReviewRecord>,
    lowerings: Vec<LoweringRecord>,
    sources: Vec<SourceRecord>,
}

impl CanonicalEncode for SeedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for SeedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for SeedPayload {
    const KIND: &'static str = SEED_KIND;
}

struct SeedCell;

impl TransitionCell for SeedCell {
    type Payload = SeedPayload;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(CharterRecord::FAMILY)?,
            RecordFamily::parse(IntentRecord::FAMILY)?,
            RecordFamily::parse(OutcomeRecord::FAMILY)?,
            RecordFamily::parse(ChangeAssessmentRecord::FAMILY)?,
            RecordFamily::parse(ChangeAdmissionRecord::FAMILY)?,
            RecordFamily::parse(ChangeBaselineRecord::FAMILY)?,
            RecordFamily::parse(ChangePolicyRecord::FAMILY)?,
            RecordFamily::parse(ChangeHoldRecord::FAMILY)?,
            RecordFamily::parse(OwnerChangeDecisionRecord::FAMILY)?,
            RecordFamily::parse(PauseRecord::FAMILY)?,
            RecordFamily::parse(ActionExceptionRecord::FAMILY)?,
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(AdaptiveReviewRecord::FAMILY)?,
            RecordFamily::parse(LoweringRecord::FAMILY)?,
            RecordFamily::parse(SourceRecord::FAMILY)?,
        ];
        affected_records.sort();
        let affected_indexes = zap_domain::viewer_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(SEED_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![RequirementRef::parse(REQ)?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        for row in &command.payload().charters {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().intents {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().outcomes {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().baselines {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().policies {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().assessments {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().admissions {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().holds {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().decisions {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().pauses {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().exceptions {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().work {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().work_replacements {
            let current = _state
                .get_typed::<WorkRecord>(&row.work_id)?
                .ok_or_else(|| test_error("work replacement source missing"))?;
            changes.replace(current.revision, row.clone())?;
        }
        for row in &command.payload().reviews {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().lowerings {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().sources {
            changes.insert(row.clone())?;
        }
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductPayload {
    work_id: WorkId,
    value: u64,
}

impl CanonicalEncode for ProductPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ProductPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for ProductPayload {
    const KIND: &'static str = PRODUCT_KIND;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductRecord {
    work_id: WorkId,
    value: u64,
    revision: Revision,
}

impl CanonicalEncode for ProductRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ProductRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl StoredRecord for ProductRecord {
    type Key = WorkId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.test.economics-product";

    fn key(&self) -> Self::Key {
        self.work_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }
}

struct ProductCell;

impl TransitionCell for ProductCell {
    type Payload = ProductPayload;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(PRODUCT_KIND)?,
            route: RouteClass::Privileged(ActionClass::parse("task.update")?),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(ProductRecord::FAMILY)?],
            affected_indexes: Vec::new(),
            requirements: vec![RequirementRef::parse(REQ)?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        if command.payload().value == u64::MAX {
            return Err(ZapError::from_static(
                ErrorCode::Conflict,
                REQ,
                "product reducer fixture failed after admission preparation",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        changes.insert(ProductRecord {
            work_id: command.payload().work_id.clone(),
            value: command.payload().value,
            revision: command.header().expected_revision().checked_next()?,
        })?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

struct ProductBasisScope;

impl PayloadBasisScope<ProductPayload> for ProductBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ProductPayload,
    ) -> Result<BasisRequest, ZapError> {
        product_basis(payload)
    }
}

fn product_basis(payload: &ProductPayload) -> Result<BasisRequest, ZapError> {
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(PRODUCT_KIND)?),
        roots: vec![SubjectRef::Work(payload.work_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::Required,
        closure: ClosureRequirement::KnownGraph,
    })
}

struct ProductImpact;

impl PayloadActionImpact<ProductPayload> for ProductImpact {
    fn request(&self, payload: &ProductPayload) -> Result<ActionImpactRequest, ZapError> {
        ActionImpactRequest::new(
            ActionImpactRule::SemanticChange,
            vec![payload.work_id.clone()],
            vec![SubjectRef::Work(payload.work_id.clone())],
        )
    }
}

struct ProductEffectContract;

impl EffectContract<ProductPayload> for ProductEffectContract {
    fn scope(
        &self,
        _state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &ProductPayload,
    ) -> Result<EffectScope, ZapError> {
        EffectScope::new(
            product_basis(payload)?,
            vec![payload.work_id.clone()],
            vec![SubjectRef::Work(payload.work_id.clone())],
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        _state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &ProductPayload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        changes.insert(ProductRecord {
            work_id: payload.work_id.clone(),
            value: payload.value,
            revision: context.observed_revision().checked_next()?,
        })
    }
}
