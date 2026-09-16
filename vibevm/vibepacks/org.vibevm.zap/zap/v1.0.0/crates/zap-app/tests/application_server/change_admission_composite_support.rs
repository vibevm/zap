use super::*;
use zap_api::{
    EffectBundleDraftInput, EffectDraftInput, PrepareCompositeSuccessorRequest,
    RecordCompositeSuccessorRequest,
};
use zap_domain::milestone_planning::{CompositeSuccessorPlanIntent, MilestonePlanProposalRecord};

pub(super) fn composite_request<T: serde::Serialize>(
    identity: &StoreIdentity,
    name: &str,
    kind: &str,
    payload: &T,
    plan_intent: CompositeSuccessorPlanIntent,
) -> Result<PrepareCompositeSuccessorRequest, ZapError> {
    Ok(PrepareCompositeSuccessorRequest {
        operation_id: OperationId::parse(&format!("operation.composite-{name}"))?,
        store: identity.clone(),
        expected_revision: Revision::new(1),
        precursors: EffectBundleDraftInput {
            alternative_id: ChangeAlternativeId::parse(&format!("alternative.composite-{name}"))?,
            committed_prefix: Vec::new(),
            effects: vec![EffectDraftInput {
                effect_id: EffectId::parse(&format!("effect.composite-{name}.0"))?,
                index: 0,
                kind: EventKind::parse(kind)?,
                payload: query_input(payload)?,
                predecessors: Vec::new(),
                product_event_id: EventId::parse(&format!("event.composite-{name}.product.0"))?,
            }],
            no_op_basis: None,
        },
        plan_intent: query_input(&plan_intent)?,
    })
}

pub(super) fn changed_request(
    request: &PrepareCompositeSuccessorRequest,
) -> Result<PrepareCompositeSuccessorRequest, ZapError> {
    let mut changed = request.clone();
    let mut intent =
        CompositeSuccessorPlanIntent::decode_canonical(&changed.plan_intent.canonical()?)?;
    intent.content.rationales[0].explanation = BoundedText::parse("Changed retry content")?;
    changed.plan_intent = query_input(&intent)?;
    Ok(changed)
}

pub(super) fn prepare(
    address: SocketAddr,
    request: &PrepareCompositeSuccessorRequest,
) -> Result<zap_api::PreparedCompositeSuccessorView, Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/prepare/composite-successor",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareCompositeSuccessor {
            request: Box::new(request.clone()),
        })?,
    )?;
    assert_eq!(status(&response)?, 200);
    let MachineResponse::PreparedCompositeSuccessor(prepared) =
        serde_json::from_slice(body(&response)?)?
    else {
        return Err("wrong composite preparation response".into());
    };
    Ok(*prepared)
}

pub(super) fn prepare_status(
    address: SocketAddr,
    request: &PrepareCompositeSuccessorRequest,
) -> Result<u16, Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/prepare/composite-successor",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareCompositeSuccessor {
            request: Box::new(request.clone()),
        })?,
    )?;
    status(&response)
}

pub(super) fn record(
    address: SocketAddr,
    prepared: zap_api::PreparedCompositeSuccessorView,
) -> Result<zap_api::RecordedCompositeSuccessorView, Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/composite-successor",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&MachineRequest::RecordCompositeSuccessor {
            request: Box::new(RecordCompositeSuccessorRequest { prepared }),
        })?,
    )?;
    assert_eq!(status(&response)?, 200);
    let MachineResponse::RecordedCompositeSuccessor(recorded) =
        serde_json::from_slice(body(&response)?)?
    else {
        return Err("wrong composite record response".into());
    };
    Ok(*recorded)
}

pub(super) fn prepared_plan_key(
    prepared: &zap_api::PreparedCompositeSuccessorView,
) -> Result<zap_domain::milestone_planning::MilestonePlanKey, ZapError> {
    Ok(MilestonePlanProposalRecord::decode_canonical(&prepared.plan.canonical()?)?.key)
}

pub(super) fn query_input<T: serde::Serialize>(value: &T) -> Result<QueryInput, ZapError> {
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, value)?;
    Ok(QueryInput {
        codec: payload.codec(),
        canonical_json: payload.as_bytes().to_vec(),
    })
}
