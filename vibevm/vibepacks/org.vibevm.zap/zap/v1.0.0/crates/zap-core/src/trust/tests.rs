use super::*;
use zap_wire::{
    BasisBinding, BoundedText, CanonicalPayload, CodecEpoch, CommandHeader, CommandHeaderInput,
    CommandId, CommandReason, CommandReasonInput, EventId, EventKind, ProtocolEpoch, Revision,
};

#[test]
fn service_seal_fences_old_handles_and_harnesses_may_host_multiple_principals()
-> Result<(), ZapError> {
    let store_id = StoreId::parse("store-trust")?;
    let campaign_id = CampaignId::parse("campaign-trust")?;
    let base_id = BaseId::parse("base-trust")?;
    let kind = EventKind::parse("runtime.observation")?;
    let frame = CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: store_id.clone(),
            campaign_id: campaign_id.clone(),
            base_id: base_id.clone(),
            command_id: CommandId::parse("command-trust")?,
            event_id: EventId::parse("event-trust")?,
            expected_revision: Revision::GENESIS,
            kind: kind.clone(),
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("trusted observation")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, b"{}")?,
    )?;
    let seal = Arc::new(());
    let mut registry = TrustRegistry::new(seal.clone());
    let mut registrar = TrustRegistrar::new(&mut registry);
    let binding = |principal: &str, observation: &str| -> Result<TrustedHostBinding, ZapError> {
        Ok(TrustedHostBinding {
            principal_id: PrincipalId::parse(principal)?,
            harness_id: HarnessId::parse("shared-harness")?,
            store_id: store_id.clone(),
            campaign_id: campaign_id.clone(),
            base_id: base_id.clone(),
            controller_epoch: ControllerEpoch::new(7)?,
            observation: ObservationRef::parse(observation)?,
            allowed_events: BTreeSet::from([kind.clone()]),
        })
    };
    let handle = registrar.bind_trusted_host(binding("driver-one", "source-one")?)?;
    let _second = registrar.bind_trusted_host(binding("driver-two", "source-two")?)?;
    let grant = handle.authorize(
        &frame,
        OperationRef::Command(frame.header().command_id().clone()),
    )?;
    assert!(grant.authorizes(&seal, &frame));
    assert!(!grant.authorizes(&Arc::new(()), &frame));
    Ok(())
}

#[test]
fn agent_data_issuer_seals_one_exact_data_proposal_frame() -> Result<(), ZapError> {
    let identity = StoreIdentity {
        store_id: StoreId::parse("store-data")?,
        campaign_id: CampaignId::parse("campaign-data")?,
        base_id: BaseId::parse("base-data")?,
        store_epoch: zap_wire::StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: zap_wire::ReducerEpoch::new(1)?,
    };
    let kind = EventKind::parse("data.proposed")?;
    let make_frame = |command: &str, event: &str| {
        CanonicalCommandFrame::new(
            CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: identity.store_id.clone(),
                campaign_id: identity.campaign_id.clone(),
                base_id: identity.base_id.clone(),
                command_id: CommandId::parse(command)?,
                event_id: EventId::parse(event)?,
                expected_revision: Revision::GENESIS,
                kind: kind.clone(),
                causes: Vec::new(),
                basis: BasisBinding::NotApplicable,
            })?,
            CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("data proposal")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, b"{}")?,
        )
    };
    let frame = make_frame("command-data", "event-data")?;
    let other = make_frame("command-other", "event-other")?;
    let seal = Arc::new(());
    let mut registry = TrustRegistry::new(seal.clone());
    let mut registrar = TrustRegistrar::new(&mut registry);
    let issuer = registrar.bind_agent_data(AgentDataBinding {
        principal_id: PrincipalId::parse("proposal-producer")?,
        store_id: identity.store_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        base_id: identity.base_id.clone(),
        allowed_events: BTreeSet::from([kind.clone()]),
    })?;
    let grant = issuer.authorize(&frame)?;
    assert!(grant.authorizes(&seal, &frame));
    assert!(!grant.authorizes(&seal, &other));
    assert_eq!(grant.actor().role, PrincipalRole::Worker);
    registry.validate(&identity)?;
    let routes = crate::RouteRegistry::single(kind, zap_wire::RouteClass::DataProposal);
    registry.validate_data_routes(&routes)?;
    Ok(())
}
