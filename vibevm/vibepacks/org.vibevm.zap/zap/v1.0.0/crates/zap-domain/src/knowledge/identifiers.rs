specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::RecordKey;
use zap_wire::{
    BoundedText, DecisionId, EvidenceId, FactId, ObligationId, OutcomeId, SourceId, SubjectRef,
    WorkId, ZapError,
};

macro_rules! knowledge_id {
    ($name:ident, $documents:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        #[specmark::spec(documents = $documents)]
        pub struct $name(BoundedText<1024>);

        impl $name {
            pub fn parse(value: &str) -> Result<Self, ZapError> {
                Ok(Self(BoundedText::parse(value)?))
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl RecordKey for $name {
            fn encode_key(&self) -> Result<Vec<u8>, ZapError> {
                Ok(self.as_str().as_bytes().to_vec())
            }
        }
    };
}

knowledge_id!(
    KnowledgeEdgeId,
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations"
);
knowledge_id!(
    RegionId,
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions"
);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations")]
pub enum KnowledgeEndpoint {
    Source(SourceId),
    Fact(FactId),
    Evidence(EvidenceId),
    Decision(DecisionId),
    Work(WorkId),
    Obligation(ObligationId),
    Outcome(OutcomeId),
}

impl KnowledgeEndpoint {
    pub fn key_bytes(&self) -> Vec<u8> {
        let (kind, id) = match self {
            Self::Source(id) => ("source", id.as_str()),
            Self::Fact(id) => ("fact", id.as_str()),
            Self::Evidence(id) => ("evidence", id.as_str()),
            Self::Decision(id) => ("decision", id.as_str()),
            Self::Work(id) => ("work", id.as_str()),
            Self::Obligation(id) => ("obligation", id.as_str()),
            Self::Outcome(id) => ("outcome", id.as_str()),
        };
        let mut bytes = Vec::with_capacity(kind.len() + id.len() + 1);
        bytes.extend_from_slice(kind.as_bytes());
        bytes.push(b':');
        bytes.extend_from_slice(id.as_bytes());
        bytes
    }

    pub fn from_subject(subject: &SubjectRef) -> Option<Self> {
        match subject {
            SubjectRef::Source(id) => Some(Self::Source(id.clone())),
            SubjectRef::Evidence(id) => Some(Self::Evidence(id.clone())),
            SubjectRef::Decision(id) => Some(Self::Decision(id.clone())),
            SubjectRef::Work(id) => Some(Self::Work(id.clone())),
            SubjectRef::Obligation(id) => Some(Self::Obligation(id.clone())),
            SubjectRef::Outcome(id) => Some(Self::Outcome(id.clone())),
            _ => None,
        }
    }

    pub fn as_subject(&self) -> Option<SubjectRef> {
        match self {
            Self::Source(id) => Some(SubjectRef::Source(id.clone())),
            Self::Evidence(id) => Some(SubjectRef::Evidence(id.clone())),
            Self::Decision(id) => Some(SubjectRef::Decision(id.clone())),
            Self::Work(id) => Some(SubjectRef::Work(id.clone())),
            Self::Obligation(id) => Some(SubjectRef::Obligation(id.clone())),
            Self::Outcome(id) => Some(SubjectRef::Outcome(id.clone())),
            Self::Fact(_) => None,
        }
    }
}

impl RecordKey for KnowledgeEndpoint {
    fn encode_key(&self) -> Result<Vec<u8>, ZapError> {
        Ok(self.key_bytes())
    }
}
