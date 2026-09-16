use std::collections::BTreeMap;
use std::sync::Arc;

use specmark::spec;
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, ErrorCode, ErrorDetail,
    FixSurface, QueryId, ZapError,
};

use crate::{Completeness, Page, QueryDescriptor, QuerySnapshot, StoreIdentity};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
);

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
/// Executes one bounded typed query against an immutable snapshot.
///
/// ```
/// use zap_core::{QuerySnapshot, QuerySpec};
/// fn run<Q: QuerySpec>(query: &Q, snapshot: &dyn QuerySnapshot, input: &Q::Input) -> Result<usize, zap_wire::ZapError> {
///     Ok(query.execute(snapshot, input)?.items.len())
/// }
/// ```
pub trait QuerySpec: Send + Sync + 'static {
    type Input: CanonicalDecode;
    type Item: CanonicalEncode;
    const ID: &'static str;

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError>;
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct ErasedQueryPage {
    pub store: StoreIdentity,
    pub revision: zap_wire::Revision,
    pub query_epoch: zap_wire::QueryEpoch,
    pub items: Vec<CanonicalOutput>,
    pub completeness: Completeness,
}

/// Preserves the descriptor while decoding a canonical query input.
///
/// ```
/// use zap_core::{ErasedQuery, QuerySnapshot};
/// fn execute(query: &dyn ErasedQuery, snapshot: &dyn QuerySnapshot, input: &zap_wire::CanonicalPayload) -> Result<usize, zap_wire::ZapError> {
///     assert_eq!(input.codec(), query.descriptor().input_codec);
///     Ok(query.decode_execute(snapshot, input)?.items.len())
/// }
/// ```
pub trait ErasedQuery: Send + Sync {
    fn descriptor(&self) -> &QueryDescriptor;
    fn decode_execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &CanonicalPayload,
    ) -> Result<ErasedQueryPage, ZapError>;
}

struct QueryAdapter<Q> {
    query: Q,
    descriptor: QueryDescriptor,
}

impl<Q: QuerySpec> ErasedQuery for QueryAdapter<Q> {
    fn descriptor(&self) -> &QueryDescriptor {
        &self.descriptor
    }

    fn decode_execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &CanonicalPayload,
    ) -> Result<ErasedQueryPage, ZapError> {
        if input.codec() != self.descriptor.input_codec {
            return Err(query_invariant());
        }
        let input = Q::Input::decode_canonical(input)?;
        let page = self.query.execute(snapshot, &input)?;
        let codec = page.store.codec_epoch;
        let items = page
            .items
            .iter()
            .map(|item| item.encode_canonical(codec))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ErasedQueryPage {
            store: page.store,
            revision: page.revision,
            query_epoch: page.query_epoch,
            items,
            completeness: page.completeness,
        })
    }
}

#[derive(Clone, Default)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct QuerySet {
    queries: BTreeMap<QueryId, Arc<dyn ErasedQuery>>,
}

impl QuerySet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn single<Q: QuerySpec>(query: Q) -> Result<Self, ZapError> {
        let id = QueryId::parse(Q::ID)?;
        let descriptor = QueryDescriptor {
            id: id.clone(),
            input_codec: zap_wire::CodecEpoch::CURRENT,
            item_codec: zap_wire::CodecEpoch::CURRENT,
            requirements: Vec::new(),
        };
        Ok(Self {
            queries: BTreeMap::from([(
                id,
                Arc::new(QueryAdapter { query, descriptor }) as Arc<dyn ErasedQuery>,
            )]),
        })
    }

    pub fn register<Q: QuerySpec>(&mut self, query: Q) -> Result<(), ZapError> {
        let set = Self::single(query)?;
        let composed = Self::compose([std::mem::take(self), set])?;
        *self = composed;
        Ok(())
    }

    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for (id, query) in set.queries {
                if result.queries.insert(id, query).is_some() {
                    return Err(duplicate_query());
                }
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }

    pub fn descriptors(&self) -> Vec<QueryDescriptor> {
        self.queries
            .values()
            .map(|query| query.descriptor().clone())
            .collect()
    }

    pub fn execute(
        &self,
        id: &QueryId,
        snapshot: &dyn QuerySnapshot,
        input: &CanonicalPayload,
    ) -> Result<ErasedQueryPage, ZapError> {
        self.queries
            .get(id)
            .ok_or_else(ZapError::unsupported_operation)?
            .decode_execute(snapshot, input)
    }
}

fn duplicate_query() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "query identity is registered more than once",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn query_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "query descriptor, codec or typed output does not match registration",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
