use zap_wire::{BoundedText, ErrorCode, ErrorDetail, FixSurface, RequirementRef, ZapError};

fn fix_label(fix: FixSurface) -> &'static str {
    match fix {
        FixSurface::Command => "command",
        FixSurface::Payload => "payload",
        FixSurface::SourceCapture => "source_capture",
        FixSurface::Authority => "authority",
        FixSurface::Policy => "policy",
        FixSurface::Store => "store",
        FixSurface::Adapter => "adapter",
        FixSurface::Configuration => "configuration",
        FixSurface::Migration => "migration",
        FixSurface::RetryAfterReconcile => "retry_after_reconcile",
    }
}

#[track_caller]
pub(crate) fn refuse<T>(
    code: ErrorCode,
    requirement: &'static str,
    why: &'static str,
    fix: FixSurface,
    detail: ErrorDetail,
) -> Result<T, ZapError> {
    let requirement = RequirementRef::parse(requirement)?;
    let message = BoundedText::parse(&format!(
        "violates REQ {}: {why}; fix surface: {}",
        requirement.as_str(),
        fix_label(fix)
    ))?;
    Err(ZapError::new(code, requirement, message, fix, detail))
}
