specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{Completeness, Page, QuerySet, QuerySnapshot, QuerySpec};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, SourceId, SubjectRef, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    ClosureStatus, EpistemicStatus, FactRecord, KnowledgeClosureRecord, RegionRecord,
    RegionRelevance, RegionState, SourceCaptureStatus, SourceRecord, SourceScope,
};
use crate::seams::{impl_canonical, scan_all, sorted_unique};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "subjects", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-basis")]
pub enum KnowledgeSummaryScope {
    All,
    Subjects(Vec<SubjectRef>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-basis")]
pub struct KnowledgeSummaryInput {
    pub scope: KnowledgeSummaryScope,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-basis")]
pub struct KnowledgeSummary {
    pub source_count: u64,
    pub fact_count: u64,
    pub region_count: u64,
    pub stale_sources: Vec<SourceId>,
    pub incomplete_subjects: Vec<SubjectRef>,
    pub relevant_unknown_regions: u64,
    pub invalidated_facts: u64,
}

impl_canonical!(KnowledgeSummaryInput);
impl_canonical!(KnowledgeSummary);

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct KnowledgeSummaryQuery;

impl QuerySpec for KnowledgeSummaryQuery {
    type Input = KnowledgeSummaryInput;
    type Item = KnowledgeSummary;
    const ID: &'static str = "zap.domain.knowledge-summary";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let selected = match &input.scope {
            KnowledgeSummaryScope::All => None,
            KnowledgeSummaryScope::Subjects(subjects) if sorted_unique(subjects) => Some(subjects),
            KnowledgeSummaryScope::Subjects(_) => {
                return Err(ZapError::from_static(
                    ErrorCode::InvalidValue,
                    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#CANVAS-DATA-ACCESS",
                    "knowledge summary subjects must be sorted and unique",
                    FixSurface::Payload,
                    ErrorDetail::None,
                ));
            }
        };
        let sources: Vec<_> = scan_all::<SourceRecord>(snapshot)?
            .into_iter()
            .filter(|source| {
                selected.is_none_or(|subjects| {
                    subjects.contains(&SubjectRef::Source(source.source_id.clone()))
                        || match &source.scope {
                            SourceScope::Project => true,
                            SourceScope::Subjects(source_subjects) => source_subjects
                                .iter()
                                .any(|subject| subjects.binary_search(subject).is_ok()),
                            SourceScope::Unassessed => false,
                        }
                })
            })
            .collect();
        let facts: Vec<_> = scan_all::<FactRecord>(snapshot)?
            .into_iter()
            .filter(|fact| {
                selected.is_none_or(|subjects| {
                    fact.subject_refs
                        .iter()
                        .any(|subject| subjects.binary_search(subject).is_ok())
                })
            })
            .collect();
        let regions: Vec<_> = scan_all::<RegionRecord>(snapshot)?
            .into_iter()
            .filter(|region| {
                selected.is_none_or(|subjects| {
                    region
                        .subject_refs
                        .iter()
                        .any(|subject| subjects.binary_search(subject).is_ok())
                        || region.work_refs.iter().any(|work_id| {
                            subjects
                                .binary_search(&SubjectRef::Work(work_id.clone()))
                                .is_ok()
                        })
                })
            })
            .collect();
        let mut stale_sources: Vec<_> = sources
            .iter()
            .filter(|source| source.capture_status != SourceCaptureStatus::Current)
            .map(|source| source.source_id.clone())
            .collect();
        stale_sources.sort();
        let closures = scan_all::<KnowledgeClosureRecord>(snapshot)?;
        let closure_by_endpoint: BTreeMap<_, _> = closures
            .iter()
            .map(|row| (row.subject.clone(), row))
            .collect();
        let mut incomplete_subjects: Vec<_> = closures
            .iter()
            .filter(|closure| closure.status != ClosureStatus::Complete)
            .filter_map(|closure| endpoint_subject(&closure.subject))
            .filter(|subject| {
                selected.is_none_or(|subjects| subjects.binary_search(subject).is_ok())
            })
            .collect();
        let known_subjects: Vec<_> = if let Some(subjects) = selected {
            subjects.clone()
        } else {
            let mut subjects = Vec::new();
            subjects.extend(
                sources
                    .iter()
                    .map(|source| SubjectRef::Source(source.source_id.clone())),
            );
            subjects.extend(
                facts
                    .iter()
                    .flat_map(|fact| fact.subject_refs.iter().cloned()),
            );
            subjects.extend(
                regions
                    .iter()
                    .flat_map(|region| region.subject_refs.iter().cloned()),
            );
            subjects.extend(
                regions
                    .iter()
                    .flat_map(|region| region.work_refs.iter().cloned().map(SubjectRef::Work)),
            );
            subjects.extend(
                scan_all::<WorkRecord>(snapshot)?
                    .into_iter()
                    .map(|work| SubjectRef::Work(work.work_id)),
            );
            subjects.extend(
                scan_all::<ObligationRecord>(snapshot)?
                    .into_iter()
                    .map(|obligation| SubjectRef::Obligation(obligation.obligation_id)),
            );
            subjects.extend(
                scan_all::<OutcomeRecord>(snapshot)?
                    .into_iter()
                    .map(|outcome| SubjectRef::Outcome(outcome.outcome_id)),
            );
            subjects.extend(
                scan_all::<EvidenceAdjudicationRecord>(snapshot)?
                    .into_iter()
                    .map(|evidence| SubjectRef::Evidence(evidence.evidence_id)),
            );
            subjects.sort();
            subjects.dedup();
            subjects
        };
        incomplete_subjects.extend(known_subjects.iter().filter_map(|subject| {
            let endpoint = crate::knowledge::KnowledgeEndpoint::from_subject(subject)?;
            closure_by_endpoint
                .get(&endpoint)
                .is_none_or(|row| row.status != ClosureStatus::Complete)
                .then(|| subject.clone())
        }));
        for fact in &facts {
            let fact_in_scope = selected.is_none_or(|subjects| {
                fact.subject_refs
                    .iter()
                    .any(|subject| subjects.binary_search(subject).is_ok())
            });
            let fact_incomplete = closure_by_endpoint
                .get(&crate::knowledge::KnowledgeEndpoint::Fact(
                    fact.fact_id.clone(),
                ))
                .is_none_or(|row| row.status != ClosureStatus::Complete);
            if fact_in_scope && fact_incomplete {
                incomplete_subjects.extend(
                    fact.subject_refs
                        .iter()
                        .filter(|subject| {
                            selected.is_none_or(|subjects| subjects.binary_search(subject).is_ok())
                        })
                        .cloned(),
                );
            }
        }
        incomplete_subjects.sort();
        incomplete_subjects.dedup();
        let summary = KnowledgeSummary {
            source_count: sources.len() as u64,
            fact_count: facts.len() as u64,
            region_count: regions.len() as u64,
            stale_sources,
            incomplete_subjects,
            relevant_unknown_regions: regions
                .iter()
                .filter(|region| {
                    region.relevance == RegionRelevance::Relevant
                        && region.state != RegionState::Evidenced
                })
                .count() as u64,
            invalidated_facts: facts
                .iter()
                .filter(|fact| fact.epistemic_status == EpistemicStatus::Invalidated)
                .count() as u64,
        };
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![summary],
            completeness: Completeness::Complete,
        })
    }
}

fn endpoint_subject(endpoint: &crate::knowledge::KnowledgeEndpoint) -> Option<SubjectRef> {
    endpoint.as_subject()
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(KnowledgeSummaryQuery)
}
