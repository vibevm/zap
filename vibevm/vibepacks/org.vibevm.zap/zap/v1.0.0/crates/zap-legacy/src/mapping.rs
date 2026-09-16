specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use std::collections::BTreeMap;

use zap_wire::{
    BaseId, CampaignId, CommandId, ContractId, DecisionId, DeferralId, DreamId, EffectId,
    ErrorCode, ErrorDetail, EventId, EvidenceId, FixSurface, HoldId, IntentId, JobId, LoweringId,
    ObligationId, OutcomeId, PauseId, ResourceId, ReviewId, SourceId, StoreId, SubjectRef,
    VerificationId, WorkId, ZapError,
};

use crate::{CurrentImportId, ImportIdMap, LegacyId, LegacyKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub enum SubjectTarget {
    Campaign,
    Intent,
    Outcome,
    Obligation,
    Work,
    Contract,
    Source,
    Evidence,
    Decision,
    Review,
    Deferral,
    Lowering,
    Dream,
    Job,
    Verification,
    Hold,
    Pause,
    Effect,
    Resource,
}

#[derive(Default)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub struct ImportMapBuilder {
    by_current: BTreeMap<CurrentImportId, LegacyId>,
    mappings: Vec<ImportIdMap>,
}

impl ImportMapBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map_subject(
        &mut self,
        legacy: LegacyId,
        target: SubjectTarget,
    ) -> Result<CurrentImportId, ZapError> {
        let spelling = deterministic_spelling(&legacy);
        let subject = match target {
            SubjectTarget::Campaign => SubjectRef::Campaign(CampaignId::parse(&spelling)?),
            SubjectTarget::Intent => SubjectRef::Intent(IntentId::parse(&spelling)?),
            SubjectTarget::Outcome => SubjectRef::Outcome(OutcomeId::parse(&spelling)?),
            SubjectTarget::Obligation => SubjectRef::Obligation(ObligationId::parse(&spelling)?),
            SubjectTarget::Work => SubjectRef::Work(WorkId::parse(&spelling)?),
            SubjectTarget::Contract => SubjectRef::Contract(ContractId::parse(&spelling)?),
            SubjectTarget::Source => SubjectRef::Source(SourceId::parse(&spelling)?),
            SubjectTarget::Evidence => SubjectRef::Evidence(EvidenceId::parse(&spelling)?),
            SubjectTarget::Decision => SubjectRef::Decision(DecisionId::parse(&spelling)?),
            SubjectTarget::Review => SubjectRef::Review(ReviewId::parse(&spelling)?),
            SubjectTarget::Deferral => SubjectRef::Deferral(DeferralId::parse(&spelling)?),
            SubjectTarget::Lowering => SubjectRef::Lowering(LoweringId::parse(&spelling)?),
            SubjectTarget::Dream => SubjectRef::Dream(DreamId::parse(&spelling)?),
            SubjectTarget::Job => SubjectRef::Job(JobId::parse(&spelling)?),
            SubjectTarget::Verification => {
                SubjectRef::Verification(VerificationId::parse(&spelling)?)
            }
            SubjectTarget::Hold => SubjectRef::Hold(HoldId::parse(&spelling)?),
            SubjectTarget::Pause => SubjectRef::Pause(PauseId::parse(&spelling)?),
            SubjectTarget::Effect => SubjectRef::Effect(EffectId::parse(&spelling)?),
            SubjectTarget::Resource => SubjectRef::Resource(ResourceId::parse(&spelling)?),
        };
        self.insert(legacy, CurrentImportId::Subject(subject))
    }

    pub fn map_event(&mut self, legacy: LegacyId) -> Result<CurrentImportId, ZapError> {
        let current = CurrentImportId::Event(EventId::parse(&deterministic_spelling(&legacy))?);
        self.insert(legacy, current)
    }

    pub fn map_command(&mut self, legacy: LegacyId) -> Result<CurrentImportId, ZapError> {
        let current = CurrentImportId::Command(CommandId::parse(&deterministic_spelling(&legacy))?);
        self.insert(legacy, current)
    }

    pub fn map_store(&mut self, legacy: LegacyId) -> Result<CurrentImportId, ZapError> {
        let current = CurrentImportId::Store(StoreId::parse(&deterministic_spelling(&legacy))?);
        self.insert(legacy, current)
    }

    pub fn map_base(&mut self, legacy: LegacyId) -> Result<CurrentImportId, ZapError> {
        let current = CurrentImportId::Base(BaseId::parse(&deterministic_spelling(&legacy))?);
        self.insert(legacy, current)
    }

    pub fn finish(mut self) -> Vec<ImportIdMap> {
        self.mappings
            .sort_by(|left, right| left.legacy.cmp(&right.legacy));
        self.mappings
    }

    fn insert(
        &mut self,
        legacy: LegacyId,
        current: CurrentImportId,
    ) -> Result<CurrentImportId, ZapError> {
        if let Some(existing) = self.by_current.get(&current) {
            if existing != &legacy {
                return Err(mapping_collision());
            }
            return Ok(current);
        }
        self.by_current.insert(current.clone(), legacy.clone());
        self.mappings.push(ImportIdMap {
            legacy,
            current: current.clone(),
        });
        Ok(current)
    }
}

pub fn deterministic_spelling(legacy: &LegacyId) -> String {
    let original = legacy.original.as_str();
    if valid_current_id(original) {
        return original.to_owned();
    }
    format!(
        "legacy:{}:{}",
        kind_name(legacy.kind),
        zap_wire::Digest32::hash(original.as_bytes()).to_hex()
    )
}

fn valid_current_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=1024).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn kind_name(kind: LegacyKind) -> &'static str {
    match kind {
        LegacyKind::Node => "node",
        LegacyKind::Mandate => "mandate",
        LegacyKind::Task => "task",
        LegacyKind::Event => "event",
        LegacyKind::Source => "source",
        LegacyKind::Other => "other",
    }
}

fn mapping_collision() -> ZapError {
    ZapError::from_static(
        ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "two distinct legacy identities map to the same typed zap/2 identity",
        FixSurface::Store,
        ErrorDetail::None,
    )
}
