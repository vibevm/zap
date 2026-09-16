use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

pub(super) fn projection_digest<R, I, H, V>(
    records: &R,
    indexes: &I,
    history: &H,
    revision_history: &V,
) -> Result<ProjectionDigest, ZapError>
where
    R: ReadableTable<&'static [u8], &'static [u8]>,
    I: ReadableTable<&'static [u8], &'static [u8]>,
    H: ReadableTable<&'static [u8], &'static [u8]>,
    V: ReadableTable<&'static [u8], &'static [u8]>,
{
    let mut digest = Sha256::new();
    for table in [
        records as &dyn ReadableBytesTable,
        indexes as &dyn ReadableBytesTable,
        history as &dyn ReadableBytesTable,
        revision_history as &dyn ReadableBytesTable,
    ] {
        table.update_digest(&mut digest)?;
    }
    Ok(ProjectionDigest::from_digest(
        zap_wire::Digest32::from_bytes(digest.finalize().into()),
    ))
}

pub(super) fn projection_digest_v2<R, I, H, S, M>(
    records: &R,
    indexes: &I,
    history_by_revision: &H,
    history_by_record: &S,
    event_meta: &M,
) -> Result<ProjectionDigest, ZapError>
where
    R: ReadableTable<&'static [u8], &'static [u8]>,
    I: ReadableTable<&'static [u8], &'static [u8]>,
    H: ReadableTable<&'static [u8], &'static [u8]>,
    S: ReadableTable<&'static [u8], &'static [u8]>,
    M: ReadableTable<u64, &'static [u8]>,
{
    let mut digest = Sha256::new();
    for table in [
        records as &dyn ReadableBytesTable,
        indexes as &dyn ReadableBytesTable,
        history_by_revision as &dyn ReadableBytesTable,
        history_by_record as &dyn ReadableBytesTable,
    ] {
        table.update_digest(&mut digest)?;
    }
    for row in event_meta.iter().map_err(|_| history_error())? {
        let (key, value) = row.map_err(|_| history_error())?;
        let key = key.value().to_be_bytes();
        digest.update((key.len() as u64).to_be_bytes());
        digest.update(key);
        digest.update((value.value().len() as u64).to_be_bytes());
        digest.update(value.value());
    }
    Ok(ProjectionDigest::from_digest(
        zap_wire::Digest32::from_bytes(digest.finalize().into()),
    ))
}

trait ReadableBytesTable {
    fn update_digest(&self, digest: &mut Sha256) -> Result<(), ZapError>;
}

impl<T> ReadableBytesTable for T
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    fn update_digest(&self, digest: &mut Sha256) -> Result<(), ZapError> {
        for row in self.iter().map_err(|_| history_error())? {
            let (key, value) = row.map_err(|_| history_error())?;
            digest.update((key.value().len() as u64).to_be_bytes());
            digest.update(key.value());
            digest.update((value.value().len() as u64).to_be_bytes());
            digest.update(value.value());
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn projection_digest_reference<R, I, H, V>(
    records: &R,
    indexes: &I,
    history: &H,
    revision_history: &V,
) -> Result<(ProjectionDigest, usize), ZapError>
where
    R: ReadableTable<&'static [u8], &'static [u8]>,
    I: ReadableTable<&'static [u8], &'static [u8]>,
    H: ReadableTable<&'static [u8], &'static [u8]>,
    V: ReadableTable<&'static [u8], &'static [u8]>,
{
    let mut bytes = Vec::new();
    for table in [
        records as &dyn ReadableBytesTableReference,
        indexes as &dyn ReadableBytesTableReference,
        history as &dyn ReadableBytesTableReference,
        revision_history as &dyn ReadableBytesTableReference,
    ] {
        table.append_all(&mut bytes)?;
    }
    Ok((ProjectionDigest::hash(&bytes), bytes.len()))
}

#[cfg(test)]
trait ReadableBytesTableReference {
    fn append_all(&self, output: &mut Vec<u8>) -> Result<(), ZapError>;
}

#[cfg(test)]
impl<T> ReadableBytesTableReference for T
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    fn append_all(&self, output: &mut Vec<u8>) -> Result<(), ZapError> {
        for row in self.iter().map_err(|_| history_error())? {
            let (key, value) = row.map_err(|_| history_error())?;
            output.extend_from_slice(&(key.value().len() as u64).to_be_bytes());
            output.extend_from_slice(key.value());
            output.extend_from_slice(&(value.value().len() as u64).to_be_bytes());
            output.extend_from_slice(value.value());
        }
        Ok(())
    }
}
