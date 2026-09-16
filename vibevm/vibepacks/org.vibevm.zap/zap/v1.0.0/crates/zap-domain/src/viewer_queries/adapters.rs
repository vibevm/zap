specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#CANVAS-DATA-ACCESS");

use zap_core::{Page, QuerySnapshot, QuerySpec};
use zap_wire::ZapError;

use super::{ViewerInput, ViewerOperation, ViewerResult, execute};

macro_rules! viewer_query {
    ($name:ident, $id:literal, $operation:expr) => {
        #[specmark::spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-query-adapters")]
        pub struct $name;
        impl QuerySpec for $name {
            type Input = ViewerInput;
            type Item = ViewerResult;
            const ID: &'static str = $id;

            fn execute(
                &self,
                snapshot: &dyn QuerySnapshot,
                input: &Self::Input,
            ) -> Result<Page<Self::Item>, ZapError> {
                execute(snapshot, input, $operation)
            }
        }
    };
}

viewer_query!(NodeQuery, "zap.viewer.node", ViewerOperation::Node);
viewer_query!(DetailQuery, "zap.viewer.detail", ViewerOperation::Detail);
viewer_query!(SearchQuery, "zap.viewer.search", ViewerOperation::Search);
viewer_query!(
    AncestorsQuery,
    "zap.viewer.ancestors",
    ViewerOperation::Ancestors
);
viewer_query!(
    ChildrenQuery,
    "zap.viewer.children",
    ViewerOperation::Children
);
viewer_query!(
    DependentsQuery,
    "zap.viewer.dependents",
    ViewerOperation::Dependents
);
viewer_query!(
    FrontierQuery,
    "zap.viewer.frontier",
    ViewerOperation::Frontier
);
viewer_query!(
    WhyBlockedQuery,
    "zap.viewer.why-blocked",
    ViewerOperation::WhyBlocked
);
viewer_query!(
    AffectedQuery,
    "zap.viewer.affected-subgraph",
    ViewerOperation::AffectedSubgraph
);
viewer_query!(
    DiffQuery,
    "zap.viewer.revision-diff",
    ViewerOperation::RevisionDiff
);
viewer_query!(HistoryQuery, "zap.viewer.history", ViewerOperation::History);
