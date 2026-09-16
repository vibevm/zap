use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
);

pub(super) fn entry_for(
    provider: &dyn BundleArtifactProvider,
    captured: Option<&BundleClosureRecord>,
    semantic_entries: Option<
        &BTreeMap<(BundleEntryKind, BoundedText<4096>), BundleArtifactCapture>,
    >,
    verify_physical: bool,
    expected: &BundleArtifactCapture,
) -> Result<BundleEntryBinding, ZapError> {
    let expected = expected.clone().validate()?;
    match captured {
        Some(closure) => {
            let entry = closure
                .manifest
                .entries
                .iter()
                .find(|entry| entry.kind == expected.kind && entry.path == expected.path)
                .ok_or_else(|| bundle_missing("captured bundle entry is missing"))?
                .clone();
            let loaded = semantic_entries
                .and_then(|entries| entries.get(&(expected.kind, expected.path.clone())))
                .ok_or_else(|| bundle_missing("captured portable semantic body is missing"))?;
            if loaded != &expected {
                return Err(bundle_conflict(
                    "captured portable body differs from the current typed record",
                ));
            }
            if verify_physical {
                provider.verify(&entry, &expected)?;
            }
            Ok(entry)
        }
        None => {
            let entry = provider.capture(&expected)?;
            if entry.kind != expected.kind || entry.path != expected.path || entry.byte_len == 0 {
                return Err(bundle_conflict(
                    "captured portable artifact changed its typed entry identity",
                ));
            }
            provider.verify(&entry, &expected)?;
            Ok(entry)
        }
    }
}

pub(super) fn semantic_capture<T: Serialize>(
    kind: BundleEntryKind,
    path: BoundedText<4096>,
    body: &T,
) -> Result<BundleArtifactCapture, ZapError> {
    BundleArtifactCapture::new(kind, path, body)
}

pub(super) fn semantic_body<T: DeserializeOwned>(
    entries: &BTreeMap<(BundleEntryKind, BoundedText<4096>), BundleArtifactCapture>,
    kind: BundleEntryKind,
    path: &BoundedText<4096>,
) -> Result<T, ZapError> {
    entries
        .get(&(kind, path.clone()))
        .ok_or_else(|| bundle_missing("portable semantic entry body is missing"))?
        .body
        .decode_json::<T>()
}

pub(super) fn semantic_entry_kind(kind: BundleEntryKind) -> bool {
    matches!(
        kind,
        BundleEntryKind::Packet
            | BundleEntryKind::Assignment
            | BundleEntryKind::Capability
            | BundleEntryKind::Permission
            | BundleEntryKind::StopRule
            | BundleEntryKind::ResultSchema
    )
}

pub(super) fn entry_path(kind: BundleEntryKind, id: &str) -> Result<BoundedText<4096>, ZapError> {
    let directory = match kind {
        BundleEntryKind::Packet => "packets",
        BundleEntryKind::Assignment => "assignments",
        BundleEntryKind::Source => "sources",
        BundleEntryKind::Rule => "rules",
        BundleEntryKind::Fork => "forks",
        BundleEntryKind::Capability => "capabilities",
        BundleEntryKind::Permission => "permissions",
        BundleEntryKind::StopRule => "stop-rules",
        BundleEntryKind::Workspace => "workspaces",
        BundleEntryKind::ResultSchema => "result-schemas",
    };
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(bundle_conflict(
            "bundle entry identity is not a lexical path segment",
        ));
    }
    BoundedText::parse(&format!("{directory}/{id}.json"))
}

pub(super) fn fork_actions(
    strategy: &StrategicPlanRecord,
    lowering: &LoweringRecord,
) -> Result<Vec<ActionClass>, ZapError> {
    lowering
        .forks
        .iter()
        .map(|ForkIdRef(id)| {
            strategy
                .forks
                .iter()
                .find(|fork| &fork.fork_id == id)
                .map(|fork| fork.selection_action.clone())
                .ok_or_else(|| bundle_missing("lowering fork is missing from strategy"))
        })
        .collect()
}

pub(super) fn one_active_charter(state: &dyn StateReader) -> Result<CharterRecord, ZapError> {
    let rows = scan_all::<CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .collect::<Vec<_>>();
    match rows.as_slice() {
        [charter] => Ok(charter.clone()),
        _ => Err(bundle_conflict(
            "bundle requires exactly one active charter",
        )),
    }
}

pub(super) fn insert_same<K: Ord, V: Eq>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
) -> Result<(), ZapError> {
    match map.entry(key) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(entry) if entry.get() == &value => {}
        std::collections::btree_map::Entry::Occupied(_) => {
            return Err(bundle_conflict(
                "bundle closure contains conflicting duplicate material",
            ));
        }
    }
    Ok(())
}

pub(super) fn scan_all<R: StoredRecord>(state: &dyn StateReader) -> Result<Vec<R>, ZapError> {
    let mut start = Bound::Unbounded;
    let mut rows = Vec::new();
    loop {
        let page = state.scan_typed::<R>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            zap_core::PageLimit::within(512, 512)?,
        )?;
        match page.completeness {
            RecordCompleteness::Complete => {
                rows.extend(page.items);
                return Ok(rows);
            }
            RecordCompleteness::More => {
                let key = page.items.last().map(StoredRecord::key).ok_or_else(|| {
                    bundle_conflict("bounded record scan omitted its continuation boundary")
                })?;
                rows.extend(page.items);
                start = Bound::Excluded(key);
            }
            RecordCompleteness::UnknownBoundary => {
                return Err(bundle_missing("bundle closure record boundary is unknown"));
            }
        }
    }
}

pub(super) fn canonical_digest(value: &impl serde::Serialize) -> Result<PayloadDigest, ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, value)?.digest())
}

pub(super) fn bundle_conflict(message: &'static str) -> ZapError {
    bundle_error(ErrorCode::Conflict, message)
}

pub(super) fn bundle_missing(message: &'static str) -> ZapError {
    bundle_error(ErrorCode::MissingReference, message)
}

pub(super) fn bundle_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        BUNDLE_REQ,
        message,
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}
