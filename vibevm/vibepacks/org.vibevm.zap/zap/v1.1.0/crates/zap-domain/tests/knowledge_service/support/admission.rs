#[derive(Clone)]
struct PreparedAdmission {
    selected: Option<EffectBundleRequest>,
    scope: Option<zap_core::AffectedScopeRequest>,
}

struct ExactAdmissions {
    admitted: Arc<Mutex<BTreeMap<CommandDigest, PreparedAdmission>>>,
    descriptor: AdmissionHookDescriptor,
}

impl ExactAdmissions {
    fn new(
        admitted: Arc<Mutex<BTreeMap<CommandDigest, PreparedAdmission>>>,
    ) -> Result<Self, ZapError> {
        Ok(Self {
            admitted,
            descriptor: AdmissionHookDescriptor::new(
                CapabilityId::parse("zap.test.exact-action-admission")?,
                ReducerEpoch::new(1)?,
                AdmissionMutationScope::new(Vec::new(), Vec::new())?,
                AdmissionMutationScope::new(Vec::new(), Vec::new())?,
            )?,
        })
    }
}

impl ActionAdmissionProvider for ExactAdmissions {
    fn descriptor(&self) -> &AdmissionHookDescriptor {
        &self.descriptor
    }

    fn needs(
        &self,
        _state: &dyn StateReader,
        _actor: &ActorRef,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError> {
        let prepared = self
            .admitted
            .lock()
            .map_err(|_| test_error())?
            .get(&request.command_digest)
            .cloned()
            .ok_or_else(test_error)?;
        ActionAdmissionNeeds::new(
            prepared.selected,
            prepared.scope.into_iter().collect(),
            Vec::new(),
            Vec::new(),
        )
    }

    fn admit(
        &self,
        _state: &dyn StateReader,
        _actor: &ActorRef,
        request: &ActionAdmissionRequest,
        preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError> {
        if !self
            .admitted
            .lock()
            .map_err(|_| test_error())?
            .contains_key(&request.command_digest)
        {
            return Err(test_error());
        }
        let basis = match request.impact.class {
            zap_core::ActionImpactClass::SemanticChange => ActionAdmissionBasis::Economic {
                admission_id: AdmissionId::parse("admission-domain-test")?,
                impact: request.impact.digest,
                selected_effect: preflight.selected_effect().ok_or_else(test_error)?.digest,
            },
            _ => ActionAdmissionBasis::Exempt {
                impact: request.impact.digest,
            },
        };
        ActionAdmissionObservation::new(
            &self.descriptor,
            basis,
            request.impact.clone(),
            &request.command_digest,
        )
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        _preflight: &ActionAdmissionPreflight<'_>,
        _changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let admitted_digest: CommandDigest = observation.decode()?;
        if admitted_digest != request.command_digest {
            return Err(test_error());
        }
        Ok(())
    }

    fn verify_after(
        &self,
        _state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        _observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        outcome: &ActionProductOutcome,
    ) -> Result<(), ZapError> {
        if request.impact.class == zap_core::ActionImpactClass::SemanticChange
            && outcome.selected_effect != preflight.selected_effect().map(|bundle| bundle.digest)
        {
            return Err(test_error());
        }
        Ok(())
    }
}
