use specmark::spec;

use serde::Serialize;
use zap_wire::{BundleDigest, EncounterDeltaDigest, PayloadDigest, ZapError};

use super::{
    BundleEntryBinding, BundleForkBinding, BundlePacketBinding, BundleRuleBinding,
    BundleSourceBinding, CharterPermissionBinding, EncounterDelta, EncounterKind, OfflineEncounter,
    StopRuleBinding, WeakBundleManifest, canonical_digest, offline_error,
};

impl WeakBundleManifest {
    pub fn seal(mut self) -> Result<Self, ZapError> {
        self.packets.sort_by(|a, b| a.packet_id.cmp(&b.packet_id));
        self.attempts.sort_by(|a, b| a.packet_id.cmp(&b.packet_id));
        self.sources.sort();
        self.rules.sort();
        self.forks.sort();
        self.capabilities.sort();
        self.permissions.sort();
        self.stop_rules.sort();
        self.entries.sort();
        if self.packets.is_empty()
            || duplicate_by(&self.packets, |row| &row.packet_id)
            || duplicate_by(&self.attempts, |row| &row.packet_id)
            || duplicates(&self.sources)
            || duplicates(&self.rules)
            || duplicates(&self.forks)
            || duplicates(&self.capabilities)
            || duplicates(&self.permissions)
            || duplicates(&self.stop_rules)
            || duplicates(&self.entries)
            || self.packets.len() != self.attempts.len()
            || self.permissions.is_empty()
            || self.capabilities.is_empty()
            || self.entries.is_empty()
            || self.maximum_archive_bytes == 0
            || self.maximum_encounters == 0
            || self.maximum_return_bytes == 0
            || self.entries.iter().any(|entry| entry.byte_len == 0)
            || self
                .entries
                .iter()
                .try_fold(0_u64, |sum, entry| sum.checked_add(entry.byte_len))
                .is_none_or(|total| total > self.maximum_archive_bytes)
            || self
                .packets
                .iter()
                .zip(&self.attempts)
                .any(|(packet, attempt)| packet.packet_id != attempt.packet_id)
        {
            return Err(offline_error(
                "bundle manifest closure or bounds are invalid",
            ));
        }
        self.encounter_genesis = encounter_genesis(&self)?;
        self.digest = bundle_manifest_digest(&self)?;
        Ok(self)
    }

    pub fn entries_digest(&self) -> Result<PayloadDigest, ZapError> {
        canonical_digest(&self.entries)
    }
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
pub fn bundle_manifest_digest(manifest: &WeakBundleManifest) -> Result<BundleDigest, ZapError> {
    #[derive(Serialize)]
    struct Body<'a> {
        bundle_id: &'a zap_wire::BundleId,
        store_id: &'a zap_wire::StoreId,
        campaign_id: &'a zap_wire::CampaignId,
        base_id: &'a zap_wire::BaseId,
        export_revision: zap_wire::Revision,
        binding: &'a super::BundleStrategyBinding,
        packets: &'a [BundlePacketBinding],
        attempts: &'a [super::BundleAttemptBinding],
        sources: &'a [BundleSourceBinding],
        rules: &'a [BundleRuleBinding],
        forks: &'a [BundleForkBinding],
        capabilities: &'a [zap_wire::CapabilityObservationId],
        permissions: &'a [CharterPermissionBinding],
        stop_rules: &'a [StopRuleBinding],
        entries: &'a [BundleEntryBinding],
        maximum_archive_bytes: u64,
        maximum_encounters: u32,
        maximum_return_bytes: u64,
        encounter_genesis: PayloadDigest,
        simulated: bool,
    }
    let body = Body {
        bundle_id: &manifest.bundle_id,
        store_id: &manifest.store_id,
        campaign_id: &manifest.campaign_id,
        base_id: &manifest.base_id,
        export_revision: manifest.export_revision,
        binding: &manifest.binding,
        packets: &manifest.packets,
        attempts: &manifest.attempts,
        sources: &manifest.sources,
        rules: &manifest.rules,
        forks: &manifest.forks,
        capabilities: &manifest.capabilities,
        permissions: &manifest.permissions,
        stop_rules: &manifest.stop_rules,
        entries: &manifest.entries,
        maximum_archive_bytes: manifest.maximum_archive_bytes,
        maximum_encounters: manifest.maximum_encounters,
        maximum_return_bytes: manifest.maximum_return_bytes,
        encounter_genesis: manifest.encounter_genesis,
        simulated: manifest.simulated,
    };
    Ok(BundleDigest::hash(
        zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, &body)?.as_bytes(),
    ))
}

fn encounter_genesis(manifest: &WeakBundleManifest) -> Result<PayloadDigest, ZapError> {
    canonical_digest(&(
        &manifest.bundle_id,
        &manifest.binding,
        manifest
            .packets
            .iter()
            .map(|row| (&row.packet_id, row.packet_digest))
            .collect::<Vec<_>>(),
        manifest
            .attempts
            .iter()
            .map(|row| (&row.job_id, &row.attempt_id, &row.packet_id))
            .collect::<Vec<_>>(),
    ))
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub struct OfflineEncounterJournal {
    manifest: WeakBundleManifest,
    encounters: Vec<OfflineEncounter>,
    previous: PayloadDigest,
    encoded_bytes: u64,
}

impl OfflineEncounterJournal {
    pub fn new(manifest: &WeakBundleManifest) -> Result<Self, ZapError> {
        if bundle_manifest_digest(manifest)? != manifest.digest {
            return Err(offline_error("offline journal requires a sealed manifest"));
        }
        Ok(Self {
            manifest: manifest.clone(),
            encounters: Vec::new(),
            previous: manifest.encounter_genesis,
            encoded_bytes: 0,
        })
    }

    pub fn append(&mut self, encounter: OfflineEncounter) -> Result<(), ZapError> {
        let sealed = encounter.clone().seal()?;
        let attempt = self
            .manifest
            .attempts
            .iter()
            .find(|attempt| attempt.job_id == sealed.job_id)
            .ok_or_else(|| offline_error("encounter job is outside the bundle"))?;
        let prior_ids = self
            .encounters
            .iter()
            .map(|row| &row.encounter_id)
            .collect::<Vec<_>>();
        let fork_valid = match sealed.kind {
            EncounterKind::ForkSelected => sealed.selected_fork.as_ref().is_some_and(|fork| {
                self.manifest
                    .forks
                    .iter()
                    .any(|binding| &binding.fork_id == fork)
            }),
            _ => sealed.selected_fork.is_none(),
        };
        if sealed.sequence != self.encounters.len() as u32
            || sealed.previous_digest != self.previous
            || sealed.attempt_id != attempt.attempt_id
            || sealed.packet_id != attempt.packet_id
            || sealed.producer != attempt.producer
            || sealed.causes.iter().any(|id| !prior_ids.contains(&id))
            || !fork_valid
            || encounter.digest != sealed.digest
        {
            return Err(offline_error(
                "encounter chain or attempt binding is invalid",
            ));
        }
        let bytes = zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, &sealed)?
            .as_bytes()
            .len() as u64;
        let next_bytes = self
            .encoded_bytes
            .checked_add(bytes)
            .ok_or_else(|| offline_error("encounter encoded byte count overflowed"))?;
        if self.encounters.len() >= self.manifest.maximum_encounters as usize
            || next_bytes > self.manifest.maximum_return_bytes
        {
            return Err(offline_error("encounter delta exceeds its exported bound"));
        }
        self.previous = sealed.digest;
        self.encoded_bytes = next_bytes;
        self.encounters.push(sealed);
        Ok(())
    }

    pub fn finish(self) -> Result<EncounterDelta, ZapError> {
        let mut delta = EncounterDelta {
            source_bundle_id: self.manifest.bundle_id,
            source_manifest_digest: self.manifest.digest,
            first_sequence: 0,
            previous_digest: self.manifest.encounter_genesis,
            final_digest: self.previous,
            encounters: self.encounters,
            encoded_bytes: self.encoded_bytes,
            digest: EncounterDeltaDigest::hash(b"pending"),
        };
        delta.digest = EncounterDeltaDigest::hash(
            zap_wire::CanonicalOutput::encode_json(
                zap_wire::CodecEpoch::CURRENT,
                &(
                    &delta.source_bundle_id,
                    delta.source_manifest_digest,
                    delta.first_sequence,
                    delta.previous_digest,
                    delta
                        .encounters
                        .iter()
                        .map(|row| row.digest)
                        .collect::<Vec<_>>(),
                    delta.final_digest,
                    delta.encoded_bytes,
                ),
            )?
            .as_bytes(),
        );
        Ok(delta)
    }
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
pub fn validate_delta(
    manifest: &WeakBundleManifest,
    delta: &EncounterDelta,
) -> Result<(), ZapError> {
    let mut journal = OfflineEncounterJournal::new(manifest)?;
    for encounter in &delta.encounters {
        journal.append(encounter.clone())?;
    }
    let rebuilt = journal.finish()?;
    if &rebuilt != delta {
        return Err(offline_error(
            "return delta does not match its exact bounded chain",
        ));
    }
    Ok(())
}

fn duplicates<T: Eq>(values: &[T]) -> bool {
    values.windows(2).any(|pair| pair[0] == pair[1])
}

fn duplicate_by<T, K: Eq>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).any(|pair| key(&pair[0]) == key(&pair[1]))
}
