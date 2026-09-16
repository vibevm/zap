specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL"
);

use zap_wire::{ArtifactDigest, ErrorCode, ErrorDetail, FixSurface, ZapError};

/// An open, preverified artifact witness retained for the duration of one commit.
///
/// ```
/// use zap_core::ArtifactWitnessGuard;
/// fn exact_witness(guard: &dyn ArtifactWitnessGuard, expected: zap_wire::ArtifactDigest) {
///     assert_eq!(guard.digests(), &[expected]);
/// }
/// ```
pub trait ArtifactWitnessGuard: Send {
    fn digests(&self) -> &[ArtifactDigest];
}

/// Fixed local provider that verifies artifact bytes before the database writer opens.
///
/// ```
/// use zap_core::ArtifactWitnessProvider;
/// fn verify(provider: &dyn ArtifactWitnessProvider, digest: zap_wire::ArtifactDigest) -> Result<(), zap_wire::ZapError> {
///     let guard = provider.prepare(&[digest])?;
///     assert_eq!(guard.digests(), &[digest]);
///     Ok(())
/// }
/// ```
pub trait ArtifactWitnessProvider: Send + Sync + 'static {
    fn prepare(
        &self,
        required: &[ArtifactDigest],
    ) -> Result<Box<dyn ArtifactWitnessGuard>, ZapError>;
}

/// Extracts the exact artifacts that must be witnessed before a command writes.
///
/// ```
/// use zap_core::{CommandPayload, PayloadArtifacts};
/// fn required<P: CommandPayload>(scope: &dyn PayloadArtifacts<P>, payload: &P) -> Result<Vec<zap_wire::ArtifactDigest>, zap_wire::ZapError> {
///     let mut digests = scope.artifacts(payload)?;
///     digests.sort();
///     Ok(digests)
/// }
/// ```
pub trait PayloadArtifacts<P: crate::CommandPayload>: Send + Sync + 'static {
    fn artifacts(&self, payload: &P) -> Result<Vec<ArtifactDigest>, ZapError>;
}

pub(crate) fn validate_artifact_digests(
    mut digests: Vec<ArtifactDigest>,
) -> Result<Vec<ArtifactDigest>, ZapError> {
    digests.sort();
    if digests.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ZapError::from_static(
            ErrorCode::DuplicateIdentity,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL",
            "artifact references must be unique",
            FixSurface::Payload,
            ErrorDetail::None,
        ));
    }
    Ok(digests)
}
