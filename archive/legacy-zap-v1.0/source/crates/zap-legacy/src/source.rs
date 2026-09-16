specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use std::path::{Path, PathBuf};

use base64::Engine;

use crate::{LegacyAuthorityClass, LegacyStoreRead, LegacyValue, Zap1Reader, unpack};

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#source-inventory")]
pub struct LegacyInventory {
    pub node_count: u64,
    pub task_count: u64,
    pub mandate_count: u64,
    pub derived_obligation_count: u64,
    pub plan_source_sha256: zap_wire::Digest32,
    pub authority: LegacyAuthorityClass,
    pub commands_executed: bool,
    pub authority_activated: bool,
}

impl LegacyInventory {
    pub fn from_base(base: &LegacyValue) -> Result<Self, crate::LegacyReadError> {
        inventory(base)
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#source-inventory")]
pub struct LegacySource {
    pub root_spelling: String,
    pub base_path_spelling: String,
    pub journal_path_spelling: String,
    pub read: LegacyStoreRead,
    pub inventory: LegacyInventory,
}

impl LegacySource {
    pub fn open(root_spelling: &str) -> Result<Self, crate::LegacyReadError> {
        let root = PathBuf::from(root_spelling);
        validate_directory(&root)?;
        let base = root.join("base.json");
        let journal = root.join("events.jsonl");
        validate_file(&base)?;
        validate_file(&journal)?;
        let base_raw =
            std::fs::read(&base).map_err(|_| source_error("BASE", "base read failed"))?;
        let journal_raw =
            std::fs::read(&journal).map_err(|_| source_error("JOURNAL", "journal read failed"))?;
        let read = Zap1Reader::read(&base_raw, &journal_raw)?;
        let inventory = inventory(&read.base)?;
        let separator = if root_spelling.contains('\\') {
            "\\"
        } else {
            "/"
        };
        let root_without_separator = root_spelling.trim_end_matches(['\\', '/']);
        Ok(Self {
            root_spelling: root_spelling.to_owned(),
            base_path_spelling: format!("{root_without_separator}{separator}base.json"),
            journal_path_spelling: format!("{root_without_separator}{separator}events.jsonl"),
            read,
            inventory,
        })
    }
}

fn inventory(base: &LegacyValue) -> Result<LegacyInventory, crate::LegacyReadError> {
    if string(field(base, "schema")?)? != "zap/1" {
        return Err(source_error("EPOCH", "unsupported legacy epoch"));
    }
    let plan = field(base, "plan")?;
    let nodes = array(field(plan, "node")?)?;
    let mandates = array(field(plan, "mandate")?)?;
    let acceptance_count = nodes.iter().try_fold(0_u64, |count, node| {
        let values = array(field(node, "acceptance")?)?;
        count
            .checked_add(values.len() as u64)
            .ok_or_else(|| source_error("LIMIT", "acceptance count overflow"))
    })?;
    let sources = field(base, "sources")?;
    let plan_source = field(sources, "plan")?;
    let plan_raw = base64::engine::general_purpose::STANDARD
        .decode(string(field(plan_source, "raw_base64")?)?)
        .map_err(|_| source_error("ENCODING", "plan source base64 is invalid"))?;
    let task_sources = array(field(sources, "tasks")?)?;
    let mut task_count = 0_u64;
    for source in task_sources {
        let raw = string(field(source, "raw_base64")?)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|_| source_error("ENCODING", "task source base64 is invalid"))?;
        let group = unpack(&bytes).map_err(|error| source_error(error.code(), error.message()))?;
        task_count = task_count
            .checked_add(array(field(&group, "tasks")?)?.len() as u64)
            .ok_or_else(|| source_error("LIMIT", "task count overflow"))?;
    }
    Ok(LegacyInventory {
        node_count: nodes.len() as u64,
        task_count,
        mandate_count: mandates.len() as u64,
        derived_obligation_count: (mandates.len() as u64)
            .checked_add(acceptance_count)
            .ok_or_else(|| source_error("LIMIT", "obligation count overflow"))?,
        plan_source_sha256: zap_wire::Digest32::hash(&plan_raw),
        authority: if matches!(field(plan, "owner_contract"), Ok(value) if !matches!(value, LegacyValue::Null))
        {
            LegacyAuthorityClass::LegacyOwnerConstraint
        } else {
            LegacyAuthorityClass::InactiveDraft
        },
        commands_executed: false,
        authority_activated: false,
    })
}

fn field<'a>(
    value: &'a LegacyValue,
    name: &str,
) -> Result<&'a LegacyValue, crate::LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(source_error("FIELDS", "expected legacy object"));
    };
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| source_error("FIELDS", "required legacy field is missing"))
}

fn array(value: &LegacyValue) -> Result<&[LegacyValue], crate::LegacyReadError> {
    let LegacyValue::Array(values) = value else {
        return Err(source_error("FIELDS", "expected legacy array"));
    };
    Ok(values)
}

fn string(value: &LegacyValue) -> Result<&str, crate::LegacyReadError> {
    let LegacyValue::String(value) = value else {
        return Err(source_error("FIELDS", "expected legacy string"));
    };
    Ok(value)
}

fn validate_directory(path: &Path) -> Result<(), crate::LegacyReadError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| source_error("SOURCE", "legacy source directory is missing"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(source_error(
            "SOURCE",
            "legacy source directory is not a plain directory",
        ));
    }
    Ok(())
}

fn validate_file(path: &Path) -> Result<(), crate::LegacyReadError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| source_error("SOURCE", "legacy source file is missing"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(source_error("SOURCE", "legacy source is not a plain file"));
    }
    Ok(())
}

fn source_error(code: &str, message: &str) -> crate::LegacyReadError {
    crate::LegacyReadError::at(code, message, 0)
}
