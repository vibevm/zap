use std::ops::Bound;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    Completeness, KeyRange, Page, PageLimit, QuerySnapshot, QuerySpec, RecordCompleteness,
    StateReaderExt,
};
use zap_wire::{
    ChangeBaselineId, ErrorCode, ErrorDetail, FixSurface, OutcomeId, PayloadDigest, PolicyId,
    Revision, ZapError,
};

use super::{ChangeBaselineRecord, ChangePolicyRecord};
use crate::intent::OutcomeRecord;
use crate::seams::{LifecycleStatus, impl_canonical};

const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#ECONOMICS-AUTHORING-CONTEXT";

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#ECONOMICS-AUTHORING-CONTEXT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct EconomicsContextInput {
    pub maximum_records: u32,
    pub maximum_candidates: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct ActiveEconomicsPolicyRef {
    pub policy_id: PolicyId,
    pub record_revision: Revision,
    pub digest: PayloadDigest,
    pub persisted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct EconomicsBaselineCandidate {
    pub baseline_id: ChangeBaselineId,
    pub record_revision: Revision,
    pub active_outcome_id: OutcomeId,
    pub change_policy_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub enum EconomicsBaselineSelection {
    Absent,
    Unique { baseline_id: ChangeBaselineId },
    Ambiguous { truncated: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#ECONOMICS-AUTHORING-CONTEXT")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct EconomicsContextView {
    pub policy: ActiveEconomicsPolicyRef,
    pub baseline_candidates: Vec<EconomicsBaselineCandidate>,
    pub baseline_selection: EconomicsBaselineSelection,
}

impl_canonical!(EconomicsContextInput);
impl_canonical!(ActiveEconomicsPolicyRef);
impl_canonical!(EconomicsBaselineCandidate);
impl_canonical!(EconomicsBaselineSelection);
impl_canonical!(EconomicsContextView);

pub(crate) struct EconomicsContextQuery;

impl QuerySpec for EconomicsContextQuery {
    type Input = EconomicsContextInput;
    type Item = EconomicsContextView;
    const ID: &'static str = "zap.economics.active-context.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        if input.maximum_candidates == 0
            || input.maximum_candidates > input.maximum_records
            || input.maximum_records > snapshot.limits().maximum_page_size
        {
            return Err(limit("economics context bounds exceed the query profile"));
        }
        let page_limit =
            PageLimit::within(input.maximum_records, snapshot.limits().maximum_page_size)?;
        let policy_page = snapshot.scan_typed::<ChangePolicyRecord>(unbounded(), page_limit)?;
        if policy_page.completeness != RecordCompleteness::Complete {
            return Err(limit(
                "active policy selection exceeds the bounded record scan",
            ));
        }
        let active = policy_page
            .items
            .into_iter()
            .filter(|policy| policy.active)
            .collect::<Vec<_>>();
        let (policy, persisted) = match active.as_slice() {
            [] => (ChangePolicyRecord::default_policy()?, false),
            [policy] => (policy.clone(), true),
            _ => return Err(conflict("more than one economics policy is active")),
        };
        let baseline_page = snapshot.scan_typed::<ChangeBaselineRecord>(unbounded(), page_limit)?;
        let scan_truncated = baseline_page.completeness != RecordCompleteness::Complete;
        let mut candidates = Vec::new();
        for baseline in baseline_page.items {
            let active_outcome = snapshot
                .get_typed::<OutcomeRecord>(&baseline.active_outcome_id)?
                .is_some_and(|outcome| outcome.status == LifecycleStatus::Active);
            if active_outcome && baseline.change_policy_revision == policy.revision {
                candidates.push(EconomicsBaselineCandidate {
                    baseline_id: baseline.baseline_id,
                    record_revision: baseline.revision,
                    active_outcome_id: baseline.active_outcome_id,
                    change_policy_revision: baseline.change_policy_revision,
                });
            }
        }
        candidates.sort_by(|left, right| left.baseline_id.cmp(&right.baseline_id));
        let candidate_count = candidates.len();
        let candidate_clipped = candidate_count > input.maximum_candidates as usize;
        let truncated = scan_truncated || candidate_clipped;
        let selection = if candidate_count == 0 && !scan_truncated {
            EconomicsBaselineSelection::Absent
        } else if candidate_count == 1 && !scan_truncated {
            EconomicsBaselineSelection::Unique {
                baseline_id: candidates[0].baseline_id.clone(),
            }
        } else {
            EconomicsBaselineSelection::Ambiguous { truncated }
        };
        if candidate_clipped {
            candidates.truncate(input.maximum_candidates as usize);
        }
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![EconomicsContextView {
                policy: ActiveEconomicsPolicyRef {
                    policy_id: policy.policy_id.clone(),
                    record_revision: policy.revision,
                    digest: policy.digest()?,
                    persisted,
                },
                baseline_candidates: candidates,
                baseline_selection: selection,
            }],
            completeness: Completeness::Complete,
        })
    }
}

fn unbounded<K>() -> KeyRange<K> {
    KeyRange {
        start: Bound::Unbounded,
        end: Bound::Unbounded,
    }
}

fn limit(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        REQUIREMENT,
        message,
        FixSurface::Command,
        ErrorDetail::None,
    )
}

fn conflict(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        REQUIREMENT,
        message,
        FixSurface::Store,
        ErrorDetail::None,
    )
}
