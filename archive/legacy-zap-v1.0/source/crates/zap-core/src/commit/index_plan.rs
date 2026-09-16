use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub struct PreparedIndexRow {
    kind: IndexMutationKind,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#prepared-mutations")]
pub enum IndexMutationKind {
    Upsert,
    Remove,
}

impl PreparedIndexRow {
    fn upsert(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            kind: IndexMutationKind::Upsert,
            key,
            value: Some(value),
        }
    }

    fn remove(key: Vec<u8>) -> Self {
        Self {
            kind: IndexMutationKind::Remove,
            key,
            value: None,
        }
    }

    pub const fn kind(&self) -> IndexMutationKind {
        self.kind
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }
}

#[derive(Default)]
struct IndexPlan {
    old: bool,
    new: Option<Vec<u8>>,
}

pub(super) fn prepare_index_rows(
    state: &dyn StateReader,
    records: &RecordSet,
    allowed: &[IndexFamily],
    mutations: &[PreparedRecordMutation],
) -> Result<Vec<PreparedIndexRow>, ZapError> {
    let mut plans = BTreeMap::<Vec<u8>, IndexPlan>::new();
    for mutation in mutations {
        if mutation.kind() != MutationKind::Insert {
            let old = state
                .get_erased(&mutation.descriptor().family, mutation.key())?
                .ok_or_else(index_invariant)?;
            for row in old.index_rows()? {
                validate_index_family(allowed, row.family())?;
                let plan = plans.entry(index_storage_key(&row)).or_default();
                if plan.old {
                    return Err(index_invariant());
                }
                plan.old = true;
            }
        }
        if let Some(value) = mutation.value() {
            let payload =
                CanonicalPayload::from_canonical_json(mutation.descriptor().value_codec, value)?;
            let new = records.decode(&mutation.descriptor().family, &payload)?;
            for row in new.index_rows()? {
                validate_index_family(allowed, row.family())?;
                let plan = plans.entry(index_storage_key(&row)).or_default();
                if plan.new.replace(row.value().to_vec()).is_some() {
                    return Err(index_invariant());
                }
            }
        }
    }
    Ok(plans
        .into_iter()
        .filter_map(|(key, plan)| {
            plan.new
                .map(|value| PreparedIndexRow::upsert(key.clone(), value))
                .or_else(|| plan.old.then(|| PreparedIndexRow::remove(key)))
        })
        .collect())
}

fn index_storage_key(row: &crate::RecordIndexRow) -> Vec<u8> {
    let family = row.family().as_str().as_bytes();
    let mut key = Vec::with_capacity(2 + family.len() + row.key().len());
    key.extend_from_slice(&(family.len() as u16).to_be_bytes());
    key.extend_from_slice(family);
    key.extend_from_slice(row.key());
    key
}

fn validate_index_family(allowed: &[IndexFamily], family: &IndexFamily) -> Result<(), ZapError> {
    if allowed.binary_search(family).is_err() {
        return Err(index_invariant());
    }
    Ok(())
}

pub(super) fn prepare_scoped_mutations(
    changes: &ChangeSet,
    records: &RecordSet,
    allowed: &[crate::RecordFamily],
) -> Result<Vec<PreparedRecordMutation>, ZapError> {
    let mutations = changes.prepare(records)?;
    if mutations.iter().any(|mutation| {
        allowed
            .binary_search(&mutation.descriptor().family)
            .is_err()
    }) {
        return Err(transaction_mismatch());
    }
    Ok(mutations)
}

pub(super) fn combine_mutation_batches(
    batches: impl IntoIterator<Item = Vec<PreparedRecordMutation>>,
) -> Result<Vec<PreparedRecordMutation>, ZapError> {
    let mut combined = BTreeMap::new();
    for mutation in batches.into_iter().flatten() {
        let key = (mutation.descriptor().family.clone(), mutation.key().clone());
        if combined.insert(key, mutation).is_some() {
            return Err(transaction_mismatch());
        }
    }
    Ok(combined.into_values().collect())
}

pub(super) fn combine_index_batches(
    batches: impl IntoIterator<Item = Vec<PreparedIndexRow>>,
) -> Result<Vec<PreparedIndexRow>, ZapError> {
    let mut combined = BTreeMap::new();
    for row in batches.into_iter().flatten() {
        if combined.insert(row.key().to_vec(), row).is_some() {
            return Err(index_invariant());
        }
    }
    Ok(combined.into_values().collect())
}
