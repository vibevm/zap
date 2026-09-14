use super::*;

pub(super) struct DreamGrillQuestionSavedCell;

impl zap_core::TransitionCell for DreamGrillQuestionSavedCell {
    type Payload = DreamGrillQuestionSaved;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        validate_question(&payload.question)?;
        let (mut questions, answers) = grill_in_progress(&branch.grill)?;
        if questions
            .iter()
            .any(|row| row.question_id == payload.question.question_id)
        {
            return Err(dream_error("grill question identity already exists"));
        }
        questions.push(payload.question.clone());
        questions.sort_by_key(|row| row.question_id);
        branch.grill = GrillState::InProgress { questions, answers };
        replace_branch(branch, changes)
    }
}

pub(super) struct DreamFactAnsweredCell;

impl zap_core::TransitionCell for DreamFactAnsweredCell {
    type Payload = DreamFactAnswered;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        if payload.source_ids.is_empty() || !sorted_unique(&payload.source_ids) {
            return Err(dream_error(
                "factual grill answer must cite sorted current sources",
            ));
        }
        for source_id in &payload.source_ids {
            let source = state
                .get_typed::<SourceRecord>(source_id)?
                .ok_or_else(|| dream_error("factual grill source is missing"))?;
            if source.capture_status != SourceCaptureStatus::Current {
                return Err(dream_error("factual grill source is stale"));
            }
        }
        let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        let (questions, mut answers) = grill_in_progress(&branch.grill)?;
        let question = question(&questions, payload.question_id)?;
        if question.kind != GrillQuestionKind::FactualDiscovery
            || question.source_ids != payload.source_ids
        {
            return Err(dream_error(
                "factual answer does not match its saved question sources",
            ));
        }
        insert_answer(
            &mut answers,
            GrillAnswer {
                question_id: payload.question_id,
                value: GrillAnswerValue::Factual {
                    statement: payload.statement.clone(),
                    source_ids: payload.source_ids.clone(),
                },
                actor: command.authority().actor().cloned(),
            },
        )?;
        branch.grill = GrillState::InProgress { questions, answers };
        replace_branch(branch, changes)
    }
}

pub(super) struct DreamOwnerAnsweredCell;

impl zap_core::TransitionCell for DreamOwnerAnsweredCell {
    type Payload = DreamOwnerAnswered;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::OwnerControl(ControlClass::ChangeDecisionRecord),
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let actor = command
            .authority()
            .actor()
            .cloned()
            .ok_or_else(|| dream_error("Owner grill answer has no authenticated actor"))?;
        let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        let (questions, mut answers) = grill_in_progress(&branch.grill)?;
        let saved = question(&questions, payload.question_id)?;
        if saved.kind == GrillQuestionKind::FactualDiscovery
            || saved
                .choices
                .iter()
                .all(|choice| choice.choice_id != payload.choice_id)
        {
            return Err(dream_error(
                "Owner answer is not one of the saved distinct choices",
            ));
        }
        if saved.kind == GrillQuestionKind::Placement {
            let selected = payload
                .resolved_attachment
                .as_ref()
                .ok_or_else(|| dream_error("placement answer omitted its exact attachment"))?;
            let DreamAttachmentState::Unresolved {
                question_id,
                candidates,
            } = &branch.attachment
            else {
                return Err(dream_error(
                    "placement answer targets an already exact attachment",
                ));
            };
            if question_id != &payload.question_id || candidates.binary_search(selected).is_err() {
                return Err(dream_error(
                    "placement answer is outside the saved candidates",
                ));
            }
            branch.attachment = DreamAttachmentState::Exact {
                attachment: selected.clone(),
            };
            let projection = project_dream(state, &branch)?;
            branch.base_revision = state.revision();
            branch.base_strategy_record_revision = projection.strategic_record_revision;
            branch.base_strategy_semantic_digest = projection.strategic_semantic_digest;
            branch.base_scope_digest = projection.scope_digest;
        } else if payload.resolved_attachment.is_some() {
            return Err(dream_error(
                "preference answer cannot change dream placement",
            ));
        }
        insert_answer(
            &mut answers,
            GrillAnswer {
                question_id: payload.question_id,
                value: GrillAnswerValue::OwnerChoice {
                    choice_id: payload.choice_id.clone(),
                    reason: payload.reason.clone(),
                },
                actor: Some(actor),
            },
        )?;
        branch.grill = GrillState::InProgress { questions, answers };
        replace_branch(branch, changes)
    }
}

pub(super) struct DreamGrillDeclinedCell;

impl zap_core::TransitionCell for DreamGrillDeclinedCell {
    type Payload = DreamGrillDeclined;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::OwnerControl(ControlClass::ChangeDecisionRecord),
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        if branch.grill != GrillState::Offered
            || !matches!(branch.attachment, DreamAttachmentState::Exact { .. })
        {
            return Err(dream_error(
                "grill decline requires an offered grill and exact attachment",
            ));
        }
        branch.grill = GrillState::Declined {
            actor: command
                .authority()
                .actor()
                .cloned()
                .ok_or_else(|| dream_error("grill decline has no authenticated Owner"))?,
        };
        branch.status = DreamStatus::Ready;
        replace_branch(branch, changes)
    }
}

pub(super) struct DreamGrillCompletedCell;

impl zap_core::TransitionCell for DreamGrillCompletedCell {
    type Payload = DreamGrillCompleted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[DreamBranchRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        let (questions, answers) = grill_in_progress(&branch.grill)?;
        if questions.is_empty()
            || questions.len() != answers.len()
            || questions.iter().any(|question| {
                answers
                    .iter()
                    .all(|row| row.question_id != question.question_id)
            })
            || !matches!(branch.attachment, DreamAttachmentState::Exact { .. })
        {
            return Err(dream_error(
                "grill cannot complete with unanswered or ambiguous questions",
            ));
        }
        let transcript =
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, &(&questions, &answers))?;
        branch.grill = GrillState::Complete {
            questions,
            answers,
            transcript_digest: GrillDigest::hash(transcript.as_bytes()),
        };
        branch.status = DreamStatus::Ready;
        replace_branch(branch, changes)
    }
}

pub(super) struct DreamRecalculatedCell;

impl zap_core::TransitionCell for DreamRecalculatedCell {
    type Payload = DreamRecalculated;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[DreamProjectionRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        let projection = project_dream(state, &branch)?;
        if let Some(current) = state.get_typed::<DreamProjectionRecord>(&payload.dream_id)? {
            changes.replace(
                current.revision,
                DreamProjectionRecord {
                    dream_id: payload.dream_id.clone(),
                    projection,
                    revision: current.revision.checked_next()?,
                },
            )?;
        } else {
            changes.insert(DreamProjectionRecord {
                dream_id: payload.dream_id.clone(),
                projection,
                revision: Revision::new(1),
            })?;
        }
        result(command)
    }
}

fn grill_in_progress(
    grill: &GrillState,
) -> Result<(Vec<GrillQuestion>, Vec<GrillAnswer>), ZapError> {
    match grill {
        GrillState::Offered => Ok((Vec::new(), Vec::new())),
        GrillState::InProgress { questions, answers } => Ok((questions.clone(), answers.clone())),
        _ => Err(dream_error("grill is not accepting questions or answers")),
    }
}

fn validate_question(question: &GrillQuestion) -> Result<(), ZapError> {
    if question.choices.len() < 2
        || !question
            .choices
            .windows(2)
            .all(|pair| pair[0].choice_id < pair[1].choice_id)
        || question.recommendation.as_ref().is_some_and(|recommended| {
            question
                .choices
                .iter()
                .all(|choice| &choice.choice_id != recommended)
        })
        || !sorted_unique(&question.source_ids)
        || (question.kind == GrillQuestionKind::FactualDiscovery && question.source_ids.is_empty())
    {
        return Err(dream_error(
            "grill question choices, recommendation or sources are invalid",
        ));
    }
    Ok(())
}

fn question(questions: &[GrillQuestion], id: GrillQuestionId) -> Result<&GrillQuestion, ZapError> {
    questions
        .iter()
        .find(|row| row.question_id == id)
        .ok_or_else(|| dream_error("grill answer references a missing question"))
}

fn insert_answer(answers: &mut Vec<GrillAnswer>, answer: GrillAnswer) -> Result<(), ZapError> {
    if answers
        .iter()
        .any(|row| row.question_id == answer.question_id)
    {
        return Err(dream_error("grill question already has a saved answer"));
    }
    answers.push(answer);
    answers.sort_by_key(|row| row.question_id);
    Ok(())
}

fn replace_branch(
    mut branch: DreamBranchRecord,
    changes: &mut ChangeSet,
) -> Result<DomainMutation, ZapError> {
    let expected = branch.revision;
    branch.revision = branch.revision.checked_next()?;
    changes.replace(expected, branch)?;
    Ok(DomainMutation {
        revision: expected.checked_next()?,
    })
}
