mod adaptive_apply;
mod adaptive_scope;
mod graph;
mod region;
mod revalidation;
mod review;
mod source;

use std::marker::PhantomData;

use zap_core::{CellSet, ChangeSet, CommandPayload, StateReader, TransitionCell, ValidatedCommand};
use zap_wire::{RouteClass, ZapError};

use crate::seams::{DomainMutation, cell_descriptor};

pub(super) trait Operation: Send + Sync + 'static {
    type Payload: CommandPayload;
    const REQUIREMENT: &'static str;
    const FAMILIES: &'static [&'static str];

    fn route() -> Result<RouteClass, ZapError>;
    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

pub(super) struct OperationCell<O>(PhantomData<O>);

impl<O> OperationCell<O> {
    pub(super) const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<O: Operation> TransitionCell for OperationCell<O> {
    type Payload = O::Payload;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            O::route()?,
            O::FAMILIES,
            O::REQUIREMENT,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        O::apply(state, command, changes)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    let mut sets = adaptive_apply::cell_sets()?;
    sets.extend(source::cell_sets()?);
    sets.extend(graph::cell_sets()?);
    sets.extend(region::cell_sets()?);
    sets.extend(review::cell_sets()?);
    sets.extend(revalidation::cell_sets()?);
    Ok(sets)
}
