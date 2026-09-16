use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, CapabilityObservationId, ErrorCode, ErrorDetail, FixSurface, HarnessId, ZapError,
};

use crate::WorkerRole;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-DEFAULT-PROFILES");

macro_rules! profile_name {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        #[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#worker-profiles")]
        pub struct $name(BoundedText<256>);

        impl $name {
            #[track_caller]
            pub fn parse(value: &str) -> Result<Self, ZapError> {
                BoundedText::parse(value).map(Self).map_err(|_| {
                    ZapError::from_static(
                        ErrorCode::InvalidValue,
                        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-DEFAULT-PROFILES",
                        concat!($label, " must be a non-empty bounded value"),
                        FixSurface::Configuration,
                        ErrorDetail::None,
                    )
                })
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
    };
}

profile_name!(ProviderName, "provider name");
profile_name!(ModelName, "model name");
profile_name!(EffortName, "effort name");

/// The Owner-selected role profile before host capability resolution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-DEFAULT-PROFILES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#worker-profiles")]
pub struct DesiredProfile {
    pub role: WorkerRole,
    pub provider: ProviderName,
    pub model: ModelName,
    pub effort: EffortName,
}

/// Explicit resolution of each desired model and effort value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#worker-profiles")]
pub enum ProfileResolution {
    Exact,
    MissingModel,
    MissingEffort,
    UnsupportedModel { desired: ModelName },
    UnsupportedEffort { desired: EffortName },
    UnavailableHost,
}

/// Actual host profile, stored separately from the desired values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-DEFAULT-PROFILES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#worker-profiles")]
pub struct ResolvedProfile {
    pub desired: DesiredProfile,
    pub harness_id: HarnessId,
    pub observation_id: CapabilityObservationId,
    pub provider: Option<ProviderName>,
    pub model: Option<ModelName>,
    pub effort: Option<EffortName>,
    pub resolution: ProfileResolution,
}

impl ResolvedProfile {
    /// Constructs a profile only when its explicit resolution agrees with actual values.
    #[track_caller]
    pub fn new(
        desired: DesiredProfile,
        harness_id: HarnessId,
        observation_id: CapabilityObservationId,
        provider: Option<ProviderName>,
        model: Option<ModelName>,
        effort: Option<EffortName>,
        resolution: ProfileResolution,
    ) -> Result<Self, ZapError> {
        let exact_values = provider.as_ref() == Some(&desired.provider)
            && model.as_ref() == Some(&desired.model)
            && effort.as_ref() == Some(&desired.effort);
        let actual_complete = provider.is_some() && model.is_some() && effort.is_some();
        if matches!(resolution, ProfileResolution::Exact) && !exact_values {
            return Err(profile_mismatch());
        }
        if !matches!(resolution, ProfileResolution::Exact) && actual_complete {
            return Err(profile_mismatch());
        }
        Ok(Self {
            desired,
            harness_id,
            observation_id,
            provider,
            model,
            effort,
            resolution,
        })
    }

    pub fn is_exact(&self) -> bool {
        matches!(self.resolution, ProfileResolution::Exact)
    }
}

fn profile_mismatch() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-DEFAULT-PROFILES",
        "resolved profile values and resolution classification disagree",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
