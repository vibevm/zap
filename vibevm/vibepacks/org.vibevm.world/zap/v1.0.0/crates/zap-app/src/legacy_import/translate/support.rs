use zap_domain::seams::WorkKind;
use zap_legacy::{
    CurrentImportId, ImportMapBuilder, LegacyId, LegacyKind, LegacyValue, SubjectTarget,
};
use zap_wire::{BoundedText, ContractId, ObligationId, SubjectRef, WorkId, ZapError};

pub(super) fn mapped_work(
    mapping: &mut ImportMapBuilder,
    kind: LegacyKind,
    id: &str,
) -> Result<WorkId, ZapError> {
    match mapping.map_subject(LegacyId::new(kind, id)?, SubjectTarget::Work)? {
        CurrentImportId::Subject(SubjectRef::Work(id)) => Ok(id),
        _ => Err(import_error()),
    }
}

pub(super) fn mapped_obligation(
    mapping: &mut ImportMapBuilder,
    kind: LegacyKind,
    id: &str,
) -> Result<ObligationId, ZapError> {
    match mapping.map_subject(LegacyId::new(kind, id)?, SubjectTarget::Obligation)? {
        CurrentImportId::Subject(SubjectRef::Obligation(id)) => Ok(id),
        _ => Err(import_error()),
    }
}

pub(super) fn mapped_contract(
    mapping: &mut ImportMapBuilder,
    kind: LegacyKind,
    id: &str,
) -> Result<ContractId, ZapError> {
    match mapping.map_subject(LegacyId::new(kind, id)?, SubjectTarget::Contract)? {
        CurrentImportId::Subject(SubjectRef::Contract(id)) => Ok(id),
        _ => Err(import_error()),
    }
}

pub(super) fn work_kind(value: &str) -> Result<WorkKind, ZapError> {
    match value {
        "portfolio" => Ok(WorkKind::Portfolio),
        "campaign" => Ok(WorkKind::Campaign),
        "phase" => Ok(WorkKind::Phase),
        "workstream" => Ok(WorkKind::Workstream),
        "group" => Ok(WorkKind::Group),
        "atom" => Ok(WorkKind::Atom),
        "gate" => Ok(WorkKind::Gate),
        "horizon" => Ok(WorkKind::Horizon),
        _ => Err(import_error()),
    }
}

pub(super) fn field<'a>(value: &'a LegacyValue, name: &str) -> Result<&'a LegacyValue, ZapError> {
    object(value)?
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(import_error)
}

pub(super) fn optional_text<'a>(
    value: &'a LegacyValue,
    name: &str,
) -> Result<Option<&'a str>, ZapError> {
    let Some(value) = object(value)?
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
    else {
        return Ok(None);
    };
    text(value).map(Some)
}

pub(super) fn object(value: &LegacyValue) -> Result<&[(String, LegacyValue)], ZapError> {
    let LegacyValue::Object(values) = value else {
        return Err(import_error());
    };
    Ok(values)
}

pub(super) fn array(value: &LegacyValue) -> Result<&[LegacyValue], ZapError> {
    let LegacyValue::Array(values) = value else {
        return Err(import_error());
    };
    Ok(values)
}

pub(super) fn text(value: &LegacyValue) -> Result<&str, ZapError> {
    let LegacyValue::String(value) = value else {
        return Err(import_error());
    };
    Ok(value)
}

pub(super) fn strings(value: &LegacyValue) -> Result<Vec<String>, ZapError> {
    array(value)?
        .iter()
        .map(|value| text(value).map(str::to_owned))
        .collect()
}

pub(super) fn bounded_strings<const N: usize>(
    value: &LegacyValue,
) -> Result<Vec<BoundedText<N>>, ZapError> {
    strings(value)?
        .iter()
        .map(|value| BoundedText::parse(value))
        .collect()
}

pub(super) fn ids<I>(value: &LegacyValue) -> Result<Vec<I>, ZapError>
where
    I: LegacyIdParse,
{
    let mut values = strings(value)?
        .iter()
        .map(|value| I::parse(value))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

pub(super) trait LegacyIdParse: Ord + Sized {
    fn parse(value: &str) -> Result<Self, ZapError>;
}

impl LegacyIdParse for WorkId {
    fn parse(value: &str) -> Result<Self, ZapError> {
        WorkId::parse(value)
    }
}

impl LegacyIdParse for ObligationId {
    fn parse(value: &str) -> Result<Self, ZapError> {
        ObligationId::parse(value)
    }
}

pub(super) fn exact_i64(value: &LegacyValue) -> Result<i64, ZapError> {
    match value {
        LegacyValue::U64(value) => i64::try_from(*value).map_err(|_| import_error()),
        LegacyValue::I64(value) => Ok(*value),
        _ => Err(import_error()),
    }
}

pub(super) fn normalized_order(value: &LegacyValue, offset: i64) -> Result<u32, ZapError> {
    let value = exact_i64(value)?
        .checked_add(offset)
        .ok_or_else(import_error)?;
    u32::try_from(value).map_err(|_| import_error())
}

pub(super) fn unknown_fields(
    value: &LegacyValue,
    known: &[&str],
) -> Result<Vec<BoundedText<128>>, ZapError> {
    let mut unknown = object(value)?
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, _)| BoundedText::parse(key))
        .collect::<Result<Vec<_>, _>>()?;
    unknown.sort();
    Ok(unknown)
}

pub(super) fn import_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "legacy base cannot be mapped losslessly into the inactive current projection",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

pub(super) fn translation_stage(why: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        why,
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}
