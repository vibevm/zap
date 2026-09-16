use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use crate::{
    EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, PageLimit,
    RecordCompleteness, RecordFamily,
};

use super::{ChangeSetOverlay, MutationKind, overlay_conflict};

pub(super) fn scan(
    overlay: &ChangeSetOverlay<'_>,
    family: &RecordFamily,
    range: EncodedKeyRange,
    limit: PageLimit,
) -> Result<ErasedRecordPage, zap_wire::ZapError> {
    let removal_count = overlay
        .mutations
        .iter()
        .filter(|((mutation_family, key), mutation)| {
            mutation_family == family
                && mutation.kind == MutationKind::Remove
                && range_contains(&range, key)
        })
        .count();
    let target = (limit.get() as usize)
        .checked_add(removal_count)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(overlay_conflict)?;
    let mut base_rows = BTreeMap::new();
    let mut next_start = range.start.clone();
    let mut base_complete = false;
    while base_rows.len() < target && !base_complete {
        let remaining = target
            .checked_sub(base_rows.len())
            .ok_or_else(overlay_conflict)?;
        let request_limit = u32::try_from(remaining).map_err(|_| overlay_conflict())?;
        let page = overlay.base.scan_erased(
            family,
            EncodedKeyRange {
                start: next_start.clone(),
                end: range.end.clone(),
            },
            PageLimit::within(request_limit, u32::MAX)?,
        )?;
        if page.items.len() > request_limit as usize {
            return Err(overlay_conflict());
        }
        let mut page_last = None;
        for row in page.items {
            if row.descriptor().family != *family {
                return Err(overlay_conflict());
            }
            let key = EncodedRecordKey::from_registered_bytes(row.key_bytes()?)?;
            if !range_contains(
                &EncodedKeyRange {
                    start: next_start.clone(),
                    end: range.end.clone(),
                },
                &key,
            ) || page_last.as_ref().is_some_and(|previous| previous >= &key)
                || base_rows.insert(key.clone(), row).is_some()
            {
                return Err(overlay_conflict());
            }
            page_last = Some(key);
        }
        match page.completeness {
            RecordCompleteness::Complete => base_complete = true,
            RecordCompleteness::More => {
                let boundary = page.last_key.ok_or_else(overlay_conflict)?;
                if page_last.as_ref() != Some(&boundary)
                    || !range_contains(
                        &EncodedKeyRange {
                            start: next_start.clone(),
                            end: range.end.clone(),
                        },
                        &boundary,
                    )
                {
                    return Err(overlay_conflict());
                }
                next_start = Bound::Excluded(boundary);
            }
            RecordCompleteness::UnknownBoundary => return Err(overlay_conflict()),
        }
    }

    for ((mutation_family, key), mutation) in &overlay.mutations {
        if mutation_family != family || !range_contains(&range, key) {
            continue;
        }
        if mutation.descriptor.family != *family || mutation.key != *key {
            return Err(overlay_conflict());
        }
        match mutation.kind {
            MutationKind::Remove => {
                base_rows.remove(key);
            }
            MutationKind::Insert | MutationKind::Replace => {
                base_rows.insert(
                    key.clone(),
                    mutation.value.clone().ok_or_else(overlay_conflict)?,
                );
            }
        }
    }

    let has_more = !base_complete || base_rows.len() > limit.get() as usize;
    let rows = base_rows
        .into_iter()
        .take(limit.get() as usize)
        .collect::<Vec<(EncodedRecordKey, Arc<dyn ErasedRecord>)>>();
    let last_key = rows.last().map(|(key, _)| key.clone());
    if has_more && last_key.is_none() {
        return Err(overlay_conflict());
    }
    Ok(ErasedRecordPage {
        items: rows.into_iter().map(|(_, row)| row).collect(),
        completeness: if has_more {
            RecordCompleteness::More
        } else {
            RecordCompleteness::Complete
        },
        last_key,
    })
}

fn range_contains(range: &EncodedKeyRange, key: &EncodedRecordKey) -> bool {
    let after_start = match &range.start {
        Bound::Included(start) => key >= start,
        Bound::Excluded(start) => key > start,
        Bound::Unbounded => true,
    };
    let before_end = match &range.end {
        Bound::Included(end) => key <= end,
        Bound::Excluded(end) => key < end,
        Bound::Unbounded => true,
    };
    after_start && before_end
}

#[cfg(test)]
#[path = "record_overlay/tests.rs"]
mod tests;
