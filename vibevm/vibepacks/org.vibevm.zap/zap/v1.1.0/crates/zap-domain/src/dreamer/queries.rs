specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{Completeness, Page, QuerySet, QuerySnapshot, QuerySpec, StateReaderExt};
use zap_wire::{DreamId, ZapError};

use crate::dreamer::{DreamBranchRecord, DreamProjection, DreamProjectionRecord, project_dream};
use crate::seams::impl_canonical;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamViewInput {
    pub dream_id: DreamId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamView {
    pub branch: DreamBranchRecord,
    pub current_projection: DreamProjection,
    pub saved_projection: Option<DreamProjectionRecord>,
}

impl_canonical!(DreamViewInput);
impl_canonical!(DreamView);

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct DreamViewQuery;

impl QuerySpec for DreamViewQuery {
    type Input = DreamViewInput;
    type Item = DreamView;
    const ID: &'static str = "zap.planning.dream";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let branch = snapshot
            .get_typed::<DreamBranchRecord>(&input.dream_id)?
            .ok_or_else(|| super::dream_error("dream query target is missing"))?;
        let current_projection = project_dream(snapshot, &branch)?;
        let saved_projection = snapshot.get_typed::<DreamProjectionRecord>(&input.dream_id)?;
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![DreamView {
                branch,
                current_projection,
                saved_projection,
            }],
            completeness: Completeness::Complete,
        })
    }
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(DreamViewQuery)
}
