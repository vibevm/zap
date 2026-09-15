use super::*;

impl<S: TransactionStore> CommitService<S> {
    #[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
    pub fn prepare_action_impact(
        &self,
        frame: &CanonicalCommandFrame,
    ) -> Result<crate::ActionImpactView, ZapError> {
        validate_frame_identity(&self.identity, frame.header())?;
        let snapshot = self.store.read(ReadAt::Current)?;
        if frame.header().expected_revision() != StateReader::revision(&snapshot).checked_next()? {
            return Err(stale_revision_error());
        }
        let cell = self
            .cells
            .cell(frame.header().kind())
            .ok_or_else(ZapError::unsupported_operation)?;
        let decoded = cell.decode_payload(frame.payload())?;
        let route = self
            .routes
            .route(frame.header().kind())
            .ok_or_else(unauthorized)?;
        let action = match route {
            RouteClass::Privileged(action) if cell.descriptor().route() == route => action,
            _ => return Err(unauthorized()),
        };
        let basis_request = cell.basis_request(&snapshot, decoded.as_ref())?;
        let relevant_basis = match (basis_request.as_ref(), frame.header().basis()) {
            (Some(request), BasisBinding::Exact(expected)) => {
                let observed = self
                    .basis
                    .as_ref()
                    .ok_or_else(stale_basis_error)?
                    .relevant_basis(&snapshot, request)?;
                if observed.digest != *expected {
                    return Err(stale_basis_error());
                }
                Some(observed)
            }
            (None, BasisBinding::NotApplicable) => None,
            _ => return Err(stale_basis_error()),
        };
        let impact_request = cell
            .action_impact_request(decoded.as_ref())?
            .ok_or_else(unauthorized)?;
        let context = crate::ActionImpactContext {
            action,
            kind: frame.header().kind(),
            event_id: frame.header().event_id(),
            payload_digest: frame.payload().digest(),
            relevant_basis: relevant_basis.as_ref(),
        };
        let impact = self
            .action_impact
            .as_ref()
            .ok_or_else(unauthorized)?
            .classify(&snapshot, &context, &impact_request)?;
        validate_action_impact(&snapshot, &context, &impact_request, &impact)?;
        Ok(impact)
    }
}
