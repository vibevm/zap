macro_rules! finish_owner_decision_journey {
    (
        identity: $identity:ident,
        service: $service:ident,
        store: $store:ident,
        internal: $internal:ident,
        owner: $owner:ident,
        coordinator: $coordinator:ident,
        provider: $provider:ident,
        cells: $cells:ident,
        records: $records:ident,
        adjudicated: $adjudicated:ident,
        effect_zero: $effect_zero:ident,
        effect_one: $effect_one:ident,
        basis_digest: $basis_digest:ident,
        product_zero: $product_zero:ident,
        product_one: $product_one:ident,
        policy: $policy:ident,
        work_zero: $work_zero:ident,
        work_one: $work_one:ident,
        hold_id: $hold_id:ident,
    ) => {{
        let identity = $identity;
        let service = $service;
        let store = $store;
        let internal = $internal;
        let owner = $owner;
        let coordinator = $coordinator;
        let provider = $provider;
        let cells = $cells;
        let records = $records;
        let adjudicated = $adjudicated;
        let effect_zero = $effect_zero;
        let effect_one = $effect_one;
        let basis_digest = $basis_digest;
        let product_zero = $product_zero;
        let product_one = $product_one;
        let policy = $policy;
        let work_zero = $work_zero;
        let work_one = $work_one;
        let hold_id = $hold_id;
        let decision_id = DecisionId::parse("decision.owner")?;
        let decision = ChangeDecisionRecorded {
            decision: OwnerChangeDecisionRecord {
                decision_id: decision_id.clone(),
                assessment_id: adjudicated.assessment_id.clone(),
                assessment_digest: assessment_digest(&adjudicated)?,
                forecast_id: None,
                forecast_digest: None,
                policy_id: policy.policy_id.clone(),
                policy_revision: policy.revision,
                recommended_alternative_id: ChangeAlternativeId::parse("alternative.service")?,
                choice: OwnerChangeChoice::Approve,
                reason: BoundedText::parse("Approve the exact ordered two-effect envelope")?,
                effect_fingerprints: vec![effect_zero.fingerprint()?, effect_one.fingerprint()?],
                effect_preflight_digests: adjudicated.alternatives[0]
                    .effects
                    .iter()
                    .filter_map(|effect| effect.preflight_digest)
                    .collect(),
                revision: Revision::new(6),
            },
        };
        let decision_frame = frame(
            &identity,
            "command.owner-decision",
            "event.owner-decision",
            Revision::new(5),
            BasisBinding::NotApplicable,
            None,
            &decision,
        )?;
        service.execute(PrincipalContext::Credentialed(&owner), decision_frame)?;

        let pause_digest = PayloadDigest::hash(b"campaign-pause-owner");
        let pause = CampaignPaused {
            pause: PauseRecord {
                pause_id: PauseId::parse("pause.owner")?,
                campaign_id: identity.campaign_id.clone(),
                scope: PauseScope::Campaign(identity.campaign_id.clone()),
                source: PauseSource::Owner,
                reason: BoundedText::parse("Demonstrate general pause precedence")?,
                charter_revision: Revision::new(1),
                status: PauseStatus::Active,
                state_digest: pause_digest,
                revision: Revision::new(7),
            },
        };
        let pause_frame = frame(
            &identity,
            "command.owner-pause",
            "event.owner-pause",
            Revision::new(6),
            BasisBinding::NotApplicable,
            None,
            &pause,
        )?;
        service.execute(PrincipalContext::Credentialed(&owner), pause_frame)?;

        let product_zero_paused = frame(
            &identity,
            "command.product-paused",
            "event.product-ordered-0",
            Revision::new(8),
            BasisBinding::Exact(basis_digest),
            Some(adjudicated.change_id.clone()),
            &product_zero,
        )?;
        let adjudicated_digest = assessment_digest(&adjudicated)?;
        let selected_alternative_id = ChangeAlternativeId::parse("alternative.service")?;
        let effect_zero_fingerprint = effect_zero.fingerprint()?;
        let task_update = ActionClass::parse("task.update")?;
        let effect_zero_impact = semantic_impact_digest(
            "task.update",
            &effect_zero.kind,
            &effect_zero.product_event_id,
            effect_zero.payload_digest,
            basis_digest,
            vec![work_zero.clone()],
            effect_zero.subjects.clone(),
        )?;
        let admission_zero = |product: &CanonicalCommandFrame| ChangeAdmissionRecord {
            change_id: adjudicated.change_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: adjudicated_digest,
            forecast_id: None,
            forecast_digest: None,
            decision_id: Some(decision_id.clone()),
            alternative_id: selected_alternative_id.clone(),
            effect_id: effect_zero.effect_id.clone(),
            effect_index: 0,
            effect_fingerprint: effect_zero_fingerprint,
            relevant_before: basis_digest,
            action: task_update.clone(),
            command_id: product.header().command_id().clone(),
            impact_digest: effect_zero_impact,
            effect_item_digest: EffectItemDigest::hash(b"pending-item"),
            effect_preflight_digest: EffectPreflightDigest::hash(b"pending-bundle"),
            payload_digest: product.payload().digest(),
            product_event_id: effect_zero.product_event_id.clone(),
            exception_id: None,
            hold_id: Some(hold_id.clone()),
            final_effect: false,
            applied_effect_ids: Vec::new(),
            applied: false,
            revision: Revision::new(1),
        };
        let out_of_order = ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                effect_id: effect_one.effect_id.clone(),
                effect_index: 1,
                effect_fingerprint: effect_one.fingerprint()?,
                payload_digest: effect_one.payload_digest,
                product_event_id: effect_one.product_event_id.clone(),
                final_effect: true,
                ..admission_zero(&product_zero_paused)
            },
        };
        let out_of_order_frame = frame(
            &identity,
            "command.admission-out-of-order",
            "event.admission-out-of-order",
            Revision::new(7),
            BasisBinding::NotApplicable,
            None,
            &out_of_order,
        )?;
        let permit = internal.authorize(
            &out_of_order_frame,
            OperationId::parse("operation.admission-out-of-order")?,
        )?;
        assert_eq!(
            service
                .execute(
                    PrincipalContext::ServiceInternal(&permit),
                    out_of_order_frame
                )
                .err()
                .map(|error| error.code),
            Some(ErrorCode::InvalidValue),
        );
        assert_eq!(store.head()?, Revision::new(7));

        let mut stale = admission_zero(&product_zero_paused);
        stale.assessment_digest = PayloadDigest::hash(b"stale-assessment");
        let stale_payload = ChangeAdmissionPrepared { admission: stale };
        let stale_frame = frame(
            &identity,
            "command.admission-stale",
            "event.admission-stale",
            Revision::new(7),
            BasisBinding::NotApplicable,
            None,
            &stale_payload,
        )?;
        let permit = internal.authorize(
            &stale_frame,
            OperationId::parse("operation.admission-stale")?,
        )?;
        assert!(
            service
                .execute(PrincipalContext::ServiceInternal(&permit), stale_frame)
                .is_err()
        );
        assert_eq!(store.head()?, Revision::new(7));

        let admission_payload = ChangeAdmissionPrepared {
            admission: admission_zero(&product_zero_paused),
        };
        let admission_frame = frame(
            &identity,
            "command.admission-0",
            "event.admission-0",
            Revision::new(7),
            BasisBinding::NotApplicable,
            None,
            &admission_payload,
        )?;
        let permit = internal.authorize(
            &admission_frame,
            OperationId::parse("operation.admission-0")?,
        )?;
        service.execute(PrincipalContext::ServiceInternal(&permit), admission_frame)?;
        assert_eq!(
            service
                .execute(
                    PrincipalContext::Credentialed(&owner),
                    product_zero_paused.clone(),
                )
                .err()
                .map(|error| error.code),
            Some(ErrorCode::Unauthorized),
        );
        assert_eq!(
            service
                .execute(
                    PrincipalContext::Credentialed(&coordinator),
                    product_zero_paused,
                )
                .err()
                .map(|error| error.code),
            Some(ErrorCode::Paused),
        );
        let snapshot = store.read(ReadAt::Current)?;
        assert!(
            !snapshot
                .get::<ChangeAdmissionRecord>(&adjudicated.change_id)?
                .ok_or_else(|| test_error("paused admission missing"))?
                .applied
        );
        drop(snapshot);

        let resume = PauseResumed {
            pause_id: PauseId::parse("pause.owner")?,
            expected_state_digest: pause_digest,
        };
        let resume_frame = frame(
            &identity,
            "command.owner-resume",
            "event.owner-resume",
            Revision::new(8),
            BasisBinding::NotApplicable,
            None,
            &resume,
        )?;
        service.execute(PrincipalContext::Credentialed(&owner), resume_frame)?;

        let product_zero_rebound = frame(
            &identity,
            "command.product-0-rebound",
            "event.product-ordered-0",
            Revision::new(10),
            BasisBinding::Exact(basis_digest),
            Some(adjudicated.change_id.clone()),
            &product_zero,
        )?;
        let rebound_payload = ChangeAdmissionPrepared {
            admission: admission_zero(&product_zero_rebound),
        };
        let rebound_frame = frame(
            &identity,
            "command.admission-0-rebound",
            "event.admission-0-rebound",
            Revision::new(9),
            BasisBinding::NotApplicable,
            None,
            &rebound_payload,
        )?;
        let permit = internal.authorize(
            &rebound_frame,
            OperationId::parse("operation.admission-0-rebound")?,
        )?;
        service.execute(PrincipalContext::ServiceInternal(&permit), rebound_frame)?;
        service.execute(
            PrincipalContext::Credentialed(&coordinator),
            product_zero_rebound,
        )?;

        let unrelated = SeedPayload {
            charters: Vec::new(),
            intents: Vec::new(),
            outcomes: Vec::new(),
            baselines: Vec::new(),
            policies: Vec::new(),
            assessments: Vec::new(),
            admissions: Vec::new(),
            holds: Vec::new(),
            decisions: Vec::new(),
            pauses: Vec::new(),
            exceptions: Vec::new(),
            work: Vec::new(),
            work_replacements: Vec::new(),
            reviews: Vec::new(),
            lowerings: Vec::new(),
            sources: Vec::new(),
        };
        let unrelated_frame = frame(
            &identity,
            "command.unrelated-churn",
            "event.unrelated-churn",
            Revision::new(11),
            BasisBinding::NotApplicable,
            None,
            &unrelated,
        )?;
        let permit = internal.authorize(
            &unrelated_frame,
            OperationId::parse("operation.unrelated-churn")?,
        )?;
        service.execute(PrincipalContext::ServiceInternal(&permit), unrelated_frame)?;

        let product_one_frame = frame(
            &identity,
            "command.product-1",
            "event.product-ordered-1",
            Revision::new(13),
            BasisBinding::Exact(basis_digest),
            Some(adjudicated.change_id.clone()),
            &product_one,
        )?;
        let admission_one = ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                command_id: product_one_frame.header().command_id().clone(),
                impact_digest: semantic_impact_digest(
                    "task.update",
                    &effect_one.kind,
                    &effect_one.product_event_id,
                    effect_one.payload_digest,
                    basis_digest,
                    vec![work_one.clone()],
                    effect_one.subjects.clone(),
                )?,
                effect_item_digest: EffectItemDigest::hash(b"pending-item"),
                effect_preflight_digest: EffectPreflightDigest::hash(b"pending-bundle"),
                payload_digest: product_one_frame.payload().digest(),
                effect_id: effect_one.effect_id.clone(),
                effect_index: 1,
                effect_fingerprint: effect_one.fingerprint()?,
                product_event_id: effect_one.product_event_id.clone(),
                final_effect: true,
                applied_effect_ids: vec![effect_zero.effect_id.clone()],
                ..admission_zero(&product_one_frame)
            },
        };
        let admission_one_frame = frame(
            &identity,
            "command.admission-1",
            "event.admission-1",
            Revision::new(12),
            BasisBinding::NotApplicable,
            None,
            &admission_one,
        )?;
        let permit = internal.authorize(
            &admission_one_frame,
            OperationId::parse("operation.admission-1")?,
        )?;
        service.execute(
            PrincipalContext::ServiceInternal(&permit),
            admission_one_frame,
        )?;
        let wrong_product = frame(
            &identity,
            "command.product-1-wrong",
            "event.product-ordered-1",
            Revision::new(13),
            BasisBinding::Exact(basis_digest),
            Some(adjudicated.change_id.clone()),
            &ProductPayload {
                work_id: work_one.clone(),
                value: 99,
            },
        )?;
        assert!(
            service
                .execute(PrincipalContext::Credentialed(&coordinator), wrong_product)
                .is_err()
        );
        let snapshot = store.read(ReadAt::Current)?;
        let waiting = snapshot
            .get::<ChangeAdmissionRecord>(&adjudicated.change_id)?
            .ok_or_else(|| test_error("waiting second admission missing"))?;
        assert!(!waiting.applied);
        assert_eq!(
            waiting.applied_effect_ids,
            vec![effect_zero.effect_id.clone()]
        );
        drop(snapshot);
        service.execute(
            PrincipalContext::Credentialed(&coordinator),
            product_one_frame,
        )?;
        let snapshot = store.read(ReadAt::Current)?;
        let complete = snapshot
            .get::<ChangeAdmissionRecord>(&adjudicated.change_id)?
            .ok_or_else(|| test_error("completed ordered admission missing"))?;
        assert!(complete.applied && complete.final_effect);
        assert_eq!(
            complete.applied_effect_ids,
            vec![effect_zero.effect_id, effect_one.effect_id],
        );
        let final_completion = CompletionEvaluator::new(
            zap_domain::completion_provider_set()?,
            vec![
                CompletionProviderId::parse("zap.domain")?,
                CompletionProviderId::parse("zap.control")?,
                CompletionProviderId::parse("zap.economics")?,
            ],
        )?
        .view(&snapshot)?;
        assert!(
            final_completion
                .blockers
                .contains(&CompletionBlocker::ActiveHold(hold_id))
        );
        drop(snapshot);
        audit_schema2(&store, &cells, &records, provider.as_ref())?;
        Ok(())
    }};
}
