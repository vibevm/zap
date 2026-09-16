use redb::TableDefinition;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

pub(crate) const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
pub(crate) const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("events");
pub(crate) const COMMANDS: TableDefinition<&str, &[u8]> = TableDefinition::new("commands");
pub(crate) const RECORDS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("records");
pub(crate) const INDEX_ROWS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_rows");
pub(crate) const RECORD_HISTORY: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("record_history_v1");
pub(crate) const REVISION_HISTORY: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("revision_history_v1");
pub(crate) const RECORDS_V2: TableDefinition<&[u8], &[u8]> = TableDefinition::new("records_v2");
pub(crate) const HISTORY_BY_REVISION_V2: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("history_by_revision_v2");
pub(crate) const HISTORY_BY_RECORD_V2: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("history_by_record_v2");
pub(crate) const HISTORY_EVENT_META_V2: TableDefinition<u64, &[u8]> =
    TableDefinition::new("history_event_meta_v2");
pub(crate) const TRAVERSAL_SESSIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("derived_traversal_sessions_v1");
pub(crate) const TRAVERSAL_VISITED: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("derived_traversal_visited_v1");
pub(crate) const TRAVERSAL_FRONTIER: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("derived_traversal_frontier_v1");
pub(crate) const TRAVERSAL_FRONTIER_MEMBERS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("derived_traversal_frontier_members_v1");
