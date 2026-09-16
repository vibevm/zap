use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use tempfile::tempdir;
use zap_core::*;
use zap_core::{
    ActionAdmissionObservationV1 as ActionAdmissionObservationCompat,
    ActionAdmissionProviderV1 as ActionAdmissionProviderCompat,
    ActionAdmissionRequestV1 as ActionAdmissionRequestCompat,
};
use zap_runtime::*;
use zap_store::{ArtifactStore, RedbStore};
use zap_wire::*;

include!("runtime_persistence/admission_support.rs");

include!("runtime_persistence/runtime_support.rs");

include!("runtime_persistence/fixture_support.rs");

include!("runtime_persistence/runtime_scale_support.rs");

include!("runtime_persistence/recovery_test.rs");

include!("runtime_persistence/native_retry_test.rs");

include!("runtime_persistence/candidate_test.rs");

fn runtime_test_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = affected_job_index_families()?;
    families.extend(runtime_index_families()?);
    families.sort();
    families.dedup();
    Ok(families)
}

fn runtime_test_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    let mut algorithms = affected_job_index_algorithms()?;
    algorithms.extend(runtime_index_algorithms()?);
    algorithms.sort();
    Ok(algorithms)
}

fn runtime_index_values<T: serde::de::DeserializeOwned, P: serde::Serialize>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
) -> Result<Vec<T>, ZapError> {
    let family = IndexFamily::parse(family)?;
    let partition = IndexPartition::new(partition)?;
    let algorithm = runtime_index_algorithms()?
        .into_iter()
        .find(|row| row.family == family)
        .map(|row| row.fingerprint)
        .ok_or_else(|| test_error("runtime test index algorithm missing"))?;
    let mut cursor = None;
    let mut values = Vec::new();
    loop {
        let page = state.scan_index(
            &IndexScanRequest::new(
                family.clone(),
                partition.clone(),
                cursor.clone(),
                PageLimit::within(256, 256)?,
            )?
            .with_algorithm(algorithm),
        )?;
        if page.catalog.covered_revision != state.revision()
            || page.catalog.algorithm(&family) != Some(algorithm)
        {
            return Err(test_error("runtime test index catalog is stale"));
        }
        for entry in page.entries {
            values.push(
                CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?
                    .decode_json()?,
            );
        }
        if page.complete {
            break;
        }
        cursor = Some(
            page.next
                .ok_or_else(|| test_error("runtime test index cursor missing"))?,
        );
    }
    Ok(values)
}

#[test]
fn job_claim_strictly_rejects_caller_supplied_meaning() -> Result<(), ZapError> {
    let payload = CanonicalPayload::from_canonical_json(
        CodecEpoch::CURRENT,
        br#"{"attempt_id":"attempt.one","dispatch_id":"dispatch.one","effect_id":"effect.one","job":{},"job_id":"job.one","packet_id":"packet.one"}"#,
    )?;
    assert!(JobClaimPayload::decode_canonical(&payload).is_err());
    Ok(())
}

#[test]
fn runtime_completion_refuses_missing_stale_and_incompatible_catalogs()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store = RedbStore::create(root.path().join("runtime-index-refusal.redb"), identity()?)?
        .with_records(record_set()?, QueryEpoch::new(1)?);
    let evaluator = CompletionEvaluator::new(
        completion_provider_set()?,
        vec![CompletionProviderId::parse("zap.runtime")?],
    )?;

    let missing = store.read(ReadAt::Current)?;
    assert_eq!(
        evaluator.view(&missing).err().map(|error| error.code),
        Some(ErrorCode::UnsupportedEpoch)
    );
    drop(missing);

    store.rebuild_indexes_v2(
        runtime_test_index_families()?,
        runtime_test_index_algorithms()?,
        Revision::GENESIS,
    )?;
    let current = store.read(ReadAt::Current)?;
    assert_eq!(
        evaluator
            .view(&RevisionSkew { inner: &current })
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Unavailable)
    );
    drop(current);

    let relevant = IndexFamily::parse("zap.runtime.relevant-job.v1")?;
    let mut incompatible = runtime_test_index_algorithms()?;
    incompatible
        .iter_mut()
        .find(|row| row.family == relevant)
        .ok_or("relevant-job algorithm missing")?
        .fingerprint = PayloadDigest::hash(b"wrong-runtime-relevant-job-algorithm");
    store.rebuild_indexes_v2(
        runtime_test_index_families()?,
        incompatible,
        Revision::GENESIS,
    )?;
    let wrong = store.read(ReadAt::Current)?;
    assert_eq!(
        evaluator.view(&wrong).err().map(|error| error.code),
        Some(ErrorCode::Unavailable)
    );
    Ok(())
}
