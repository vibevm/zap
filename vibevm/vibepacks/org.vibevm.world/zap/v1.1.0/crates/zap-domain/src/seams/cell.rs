pub(crate) fn cell_descriptor(
    kind: &'static str,
    route: zap_wire::RouteClass,
    affected_records: &[&'static str],
    requirement: &'static str,
    requires_completion: bool,
) -> Result<zap_core::CellDescriptor, zap_wire::ZapError> {
    let mut records = affected_records
        .iter()
        .map(|family| zap_core::RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort();
    records.dedup();
    let indexes = crate::viewer_indexes::viewer_index_families_for_records(&records)?;
    zap_core::CellDescriptor::new(zap_core::CellDescriptorInput {
        kind: zap_wire::EventKind::parse(kind)?,
        route,
        payload_codec: zap_wire::CodecEpoch::CURRENT,
        reducer_epoch: zap_wire::ReducerEpoch::new(1)?,
        affected_records: records,
        affected_indexes: indexes,
        requirements: vec![zap_wire::RequirementRef::parse(requirement)?],
        requires_completion,
    })
}

macro_rules! impl_command_payload {
    ($payload:ty, $kind:literal) => {
        impl zap_core::CommandPayload for $payload {
            const KIND: &'static str = $kind;
        }
    };
}

pub(crate) use impl_command_payload;
