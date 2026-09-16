use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn export_ready_bundle(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    provider: &ApplicationBundleClosureProvider,
    capture_root: &std::path::Path,
    harness_id: &HarnessId,
    internal: &InternalProtocolHandle,
    trusted: &TrustedHostHandle,
    portable: &PortableBundleArtifactProvider,
    bundle_id: &str,
    suffix: &str,
) -> Result<WeakBundleRecord, Box<dyn std::error::Error>> {
    let request = BundleClosureRequest::new(
        BundleId::parse(bundle_id)?,
        LoweringId::parse("lowering.one")?,
        vec![PacketId::parse("packet.one")?],
        1_000_000,
        16,
        1_000_000,
        true,
    )?;
    let closure = provider.prepare(&store.read(ReadAt::Current)?, &request)?;
    let export_frame = frame(
        identity,
        &BundleExported {
            schema: BundleExportedSchema::V1,
            request: request.clone(),
            closure: closure.clone(),
        },
        store.head()?,
        &format!("command.bundle.{suffix}"),
    )?;
    provider.prepare_captured_evidence(
        &store.read(ReadAt::Current)?,
        &export_frame,
        &request,
        &closure,
    )?;
    let export_permit = internal.authorize(
        &export_frame,
        OperationId::parse(&format!("bundle.export:{suffix}"))?,
    )?;
    service.submit(
        PrincipalContext::ServiceInternal(&export_permit),
        export_frame,
    )?;
    let published = portable.publish_archive(
        &closure.manifest,
        harness_id.clone(),
        ObservationRef::parse("observation.packet-resolution")?,
    )?;
    let neutral_path = capture_root.join(format!("neutral-bundle-{suffix}.zapbundle"));
    std::fs::copy(&published.path, &neutral_path)?;
    let verified = portable.verify_archive(&neutral_path, &closure.manifest)?;
    assert_eq!(verified.manifest, closure.manifest);
    assert_eq!(verified.entries.len(), closure.manifest.entries.len());
    let packet_entry = verified
        .entry(BundleEntryKind::Packet, "packets/packet.one.json")
        .ok_or("neutral archive packet body missing")?;
    let PortableBundleEntryBody::Packet(packet_body) = &packet_entry.body else {
        return Err("neutral archive packet body has the wrong type".into());
    };
    assert_eq!(packet_body.packet.packet_id, PacketId::parse("packet.one")?);
    assert_eq!(
        packet_body.claim.job_id,
        JobId::parse("job.packet-resolution")?
    );
    let assignment = verified
        .entry(BundleEntryKind::Assignment, "assignments/work.leaf.json")
        .ok_or("neutral archive assignment body missing")?;
    let PortableBundleEntryBody::Assignment(assignment_body) = &assignment.body else {
        return Err("neutral archive assignment body has the wrong type".into());
    };
    assert_eq!(
        assignment_body.contract.contract_id,
        assignment_body.job.contract_id
    );
    assert_eq!(
        assignment_body.contract.contract_digest,
        assignment_body.job.contract_digest
    );
    let result_schema = verified
        .entry(
            BundleEntryKind::ResultSchema,
            "result-schemas/packet.one.json",
        )
        .ok_or("neutral archive result schema missing")?;
    let PortableBundleEntryBody::ResultSchema(result_contract) = &result_schema.body else {
        return Err("neutral archive result schema has the wrong type".into());
    };
    assert_eq!(result_contract.contract_id, assignment_body.job.contract_id);
    let raw_rule = verified
        .entries
        .iter()
        .find(|entry| entry.binding.kind == BundleEntryKind::Rule)
        .ok_or("neutral archive raw rule material missing")?;
    assert!(
        matches!(&raw_rule.body, PortableBundleEntryBody::Material(bytes) if !bytes.is_empty())
    );

    let mut collision = closure.manifest.clone();
    collision.bundle_id = BundleId::parse(&format!("bundle.collision.{suffix}"))?;
    let collision = collision.seal()?;
    let collision_path = capture_root
        .parent()
        .ok_or("capture root parent missing")?
        .join("portable-archives")
        .join(format!("{}.zapbundle", collision.bundle_id.as_str()));
    let foreign = b"foreign partial archive";
    std::fs::write(&collision_path, foreign)?;
    assert!(
        portable
            .publish_archive(
                &collision,
                harness_id.clone(),
                ObservationRef::parse("observation.packet-resolution")?,
            )
            .is_err()
    );
    assert_eq!(std::fs::read(collision_path)?, foreign);
    let archive_frame = frame(
        identity,
        &BundleArchivePublished {
            schema: BundleArchivePublishedSchema::V1,
            receipt: published.receipt,
        },
        store.head()?,
        &format!("command.bundle-archive.{suffix}"),
    )?;
    let archive_grant = trusted.authorize(
        &archive_frame,
        OperationRef::Command(archive_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&archive_grant),
        archive_frame,
    )?;
    let ready = store
        .read(ReadAt::Current)?
        .get_typed::<WeakBundleRecord>(&request.bundle_id)?
        .ok_or("ready bundle missing")?;
    assert_eq!(ready.status, BundleStatus::Ready);
    Ok(ready)
}
