use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ActionClass, AdmissionId, AttemptId, AuthorizationRef, CommandId, ControlClass, HarnessId,
    ObservationRef, OperationId, PrincipalId, SemanticRequestId,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION"
);

/// Execution responsibility, separate from mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub enum WorkerRole {
    Senior,
    Middle,
    Junior,
}

/// Trusted principal authority categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub enum PrincipalRole {
    Reader,
    Worker,
    Coordinator,
    Owner,
    TrustedHost,
}

/// The logical operation performed by an admitted actor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub enum OperationRef {
    Command(CommandId),
    Attempt(AttemptId),
    SemanticRequest(SemanticRequestId),
}

/// A nonsecret actor and operation identity used for separation checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub struct ActorRef {
    pub principal_id: PrincipalId,
    pub operation: OperationRef,
    pub role: PrincipalRole,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AuthorityKindV1 {
    AgentData,
    OwnerControl {
        class: ControlClass,
        reference: AuthorizationRef,
    },
    Privileged {
        action: ActionClass,
        admission: AdmissionId,
    },
    TrustedObservation {
        source: ObservationRef,
        harness: HarnessId,
    },
    ServiceInternal {
        operation: OperationId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub struct AdmittedAuthorityV1 {
    actor: Option<ActorRef>,
    kind: AuthorityKindV1,
}

impl AdmittedAuthorityV1 {
    pub fn privileged_action(&self) -> Option<(&ActionClass, &AdmissionId)> {
        match &self.kind {
            AuthorityKindV1::Privileged { action, admission } => Some((action, admission)),
            _ => None,
        }
    }

    pub(crate) fn to_runtime(&self) -> AdmittedAuthority {
        let kind = match &self.kind {
            AuthorityKindV1::AgentData => AuthorityKind::AgentData,
            AuthorityKindV1::OwnerControl { class, reference } => AuthorityKind::OwnerControl {
                class: *class,
                reference: reference.clone(),
            },
            AuthorityKindV1::Privileged { .. } => AuthorityKind::Schema1Privileged,
            AuthorityKindV1::TrustedObservation { source, harness } => {
                AuthorityKind::TrustedObservation {
                    source: source.clone(),
                    harness: harness.clone(),
                }
            }
            AuthorityKindV1::ServiceInternal { operation } => AuthorityKind::ServiceInternal {
                operation: operation.clone(),
            },
        };
        AdmittedAuthority {
            actor: self.actor.clone(),
            kind,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum AuthorityKind {
    AgentData,
    OwnerControl {
        class: ControlClass,
        reference: AuthorizationRef,
    },
    Privileged {
        action: ActionClass,
        basis: crate::ActionAdmissionBasis,
    },
    #[serde(skip)]
    Schema1Privileged,
    TrustedObservation {
        source: ObservationRef,
        harness: HarnessId,
    },
    ServiceInternal {
        operation: OperationId,
    },
}

/// Public admitted authority facts whose construction remains service-private.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#actor-authority-facts")]
pub struct AdmittedAuthority {
    actor: Option<ActorRef>,
    kind: AuthorityKind,
}

impl AdmittedAuthority {
    /// Returns the actor for non-internal admitted operations.
    pub fn actor(&self) -> Option<&ActorRef> {
        self.actor.as_ref()
    }

    pub fn owner_control_details(&self) -> Option<(ControlClass, &AuthorizationRef)> {
        match &self.kind {
            AuthorityKind::OwnerControl { class, reference } => Some((*class, reference)),
            _ => None,
        }
    }

    pub fn privileged_action(&self) -> Option<(&ActionClass, &crate::ActionAdmissionBasis)> {
        match &self.kind {
            AuthorityKind::Privileged { action, basis } => Some((action, basis)),
            _ => None,
        }
    }

    pub fn observation_source(&self) -> Option<&ObservationRef> {
        match &self.kind {
            AuthorityKind::TrustedObservation { source, .. } => Some(source),
            _ => None,
        }
    }

    pub fn observation_harness(&self) -> Option<&HarnessId> {
        match &self.kind {
            AuthorityKind::TrustedObservation { harness, .. } => Some(harness),
            _ => None,
        }
    }

    pub fn service_operation(&self) -> Option<&OperationId> {
        match &self.kind {
            AuthorityKind::ServiceInternal { operation } => Some(operation),
            _ => None,
        }
    }

    pub(crate) fn agent_data(actor: ActorRef) -> Self {
        Self {
            actor: Some(actor),
            kind: AuthorityKind::AgentData,
        }
    }

    pub(crate) fn owner_control(
        actor: ActorRef,
        class: ControlClass,
        reference: AuthorizationRef,
    ) -> Self {
        Self {
            actor: Some(actor),
            kind: AuthorityKind::OwnerControl { class, reference },
        }
    }

    pub(crate) fn privileged(
        actor: ActorRef,
        action: ActionClass,
        basis: crate::ActionAdmissionBasis,
    ) -> Self {
        Self {
            actor: Some(actor),
            kind: AuthorityKind::Privileged { action, basis },
        }
    }

    pub(crate) fn trusted_observation(
        actor: ActorRef,
        source: ObservationRef,
        harness: HarnessId,
    ) -> Self {
        Self {
            actor: Some(actor),
            kind: AuthorityKind::TrustedObservation { source, harness },
        }
    }

    pub(crate) fn service_internal(operation: OperationId) -> Self {
        Self {
            actor: None,
            kind: AuthorityKind::ServiceInternal { operation },
        }
    }
}
