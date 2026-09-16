use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(OperationCell::<ReplaceContract>::new())
            .basis(OperationBasisScope::<ReplaceContract>::new())?
            .effect_contract(OperationEffectContract::<ReplaceContract>::new())?
            .action_impact(TypedActionImpact::new(|payload: &TaskContractReplaced| {
                work_impact(ActionImpactRule::SemanticChange, &payload.work_id)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<RenameWork>::new())
            .action_impact(TypedActionImpact::new(|payload: &WorkRenamed| {
                work_impact(ActionImpactRule::Progress, &payload.work_id)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<TransitionWork>::new())
            .basis(OperationBasisScope::<TransitionWork>::new())?
            .effect_contract(OperationEffectContract::<TransitionWork>::new())?
            .action_impact(TypedActionImpact::new(|payload: &WorkTransitioned| {
                let rule = if matches!(
                    payload.to_state,
                    crate::seams::WorkState::Dropped
                        | crate::seams::WorkState::Superseded
                        | crate::seams::WorkState::Deferred
                ) {
                    ActionImpactRule::SemanticChange
                } else {
                    ActionImpactRule::Progress
                };
                work_impact(rule, &payload.work_id)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<DispatchWork>::new())
            .action_impact(TypedActionImpact::new(|payload: &WorkDispatched| {
                work_impact(ActionImpactRule::Progress, &payload.work_id)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<CreateDeferral>::new())
            .basis(OperationBasisScope::<CreateDeferral>::new())?
            .effect_contract(OperationEffectContract::<CreateDeferral>::new())?
            .action_impact(TypedActionImpact::new(|payload: &DeferralCreated| {
                ActionImpactRequest::new(
                    ActionImpactRule::SemanticChange,
                    payload.work_ids.clone(),
                    vec![zap_wire::SubjectRef::Deferral(payload.deferral_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<TransferDeferral>::new())
            .basis(OperationBasisScope::<TransferDeferral>::new())?
            .effect_contract(OperationEffectContract::<TransferDeferral>::new())?
            .action_impact(TypedActionImpact::new(|payload: &DeferralTransferred| {
                ActionImpactRequest::new(
                    ActionImpactRule::SemanticChange,
                    payload.to_work_ids.clone(),
                    vec![zap_wire::SubjectRef::Deferral(payload.deferral_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<CloseDeferral>::new())
            .action_impact(TypedActionImpact::new(|payload: &DeferralClosed| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    Vec::new(),
                    vec![zap_wire::SubjectRef::Deferral(payload.deferral_id.clone())],
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<InapplicableDeferral>::new())
            .basis(OperationBasisScope::<InapplicableDeferral>::new())?
            .effect_contract(OperationEffectContract::<InapplicableDeferral>::new())?
            .action_impact(TypedActionImpact::new(|payload: &DeferralInapplicable| {
                ActionImpactRequest::new(
                    ActionImpactRule::SemanticChange,
                    Vec::new(),
                    vec![zap_wire::SubjectRef::Deferral(payload.deferral_id.clone())],
                )
            }))?
            .build()?,
    ])
}

pub fn schema1_control_cell_set() -> Result<CellSet, ZapError> {
    CellRegistrationBuilder::new(OperationCell::<LowerPlan>::new())
        .action_impact(TypedActionImpact::new(|payload: &PlanLowered| {
            let mut work = vec![payload.parent_id.clone(), payload.integration_owner.clone()];
            work.extend(payload.nodes.iter().map(|row| row.work_id.clone()));
            ActionImpactRequest::new(
                ActionImpactRule::SemanticChange,
                work,
                vec![zap_wire::SubjectRef::Work(payload.parent_id.clone())],
            )
        }))?
        .build()
}

fn work_impact(
    rule: ActionImpactRule,
    work_id: &zap_wire::WorkId,
) -> Result<ActionImpactRequest, ZapError> {
    ActionImpactRequest::new(
        rule,
        vec![work_id.clone()],
        vec![zap_wire::SubjectRef::Work(work_id.clone())],
    )
}

pub(super) fn effect_contract_missing() -> ZapError {
    ZapError::from_static(
        ErrorCode::UnsupportedOperation,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW",
        "this operation is not a registered semantic effect",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
