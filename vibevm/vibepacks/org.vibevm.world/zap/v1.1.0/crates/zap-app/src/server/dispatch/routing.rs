use zap_api::MachineRequest;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

pub(super) fn request_matches_route(path: &str, request: &MachineRequest) -> bool {
    matches!(
        (path, request),
        ("/v1/query", MachineRequest::Query { .. })
            | ("/v1/events", MachineRequest::Events { .. })
            | ("/v1/stream", MachineRequest::Events { .. })
            | ("/v1/command", MachineRequest::Command { .. })
            | ("/v1/control", MachineRequest::Control { .. })
            | ("/v1/observation", MachineRequest::Observation { .. })
            | ("/v1/agent", MachineRequest::Agent { .. })
            | ("/v1/runtime/step", MachineRequest::RuntimeStep)
            | ("/v1/runtime/run", MachineRequest::RuntimeRun { .. })
            | ("/v1/runtime/inspect", MachineRequest::RuntimeInspect { .. })
            | ("/v1/native", MachineRequest::NativeDriver { .. })
            | (
                "/v1/prepare/bundle",
                MachineRequest::PrepareEffectBundle { .. }
            )
            | (
                "/v1/prepare/comparison",
                MachineRequest::PrepareEffectComparison { .. }
            )
            | (
                "/v1/prepare/projected-record",
                MachineRequest::PrepareProjectedRecord { .. }
            )
            | (
                "/v1/archive/publish",
                MachineRequest::PublishBundleArchive { .. }
            )
            | (
                "/v1/archive/verify",
                MachineRequest::VerifyBundleArchive { .. }
            )
            | ("/v1/archive/entry", MachineRequest::ReadBundleEntry { .. })
            | ("/v1/reconcile", MachineRequest::Reconcile { .. })
            | ("/v1/indexes/rebuild", MachineRequest::RebuildIndexes { .. })
            | (
                "/v1/traversal/begin",
                MachineRequest::BeginAffectedTraversal { .. }
            )
            | (
                "/v1/traversal/continue",
                MachineRequest::ContinueAffectedTraversal { .. }
            )
            | (
                "/v1/traversal/cancel",
                MachineRequest::CancelAffectedTraversal { .. }
            )
    )
}

pub(super) fn is_reader_request(request: &MachineRequest) -> bool {
    matches!(
        request,
        MachineRequest::Capabilities
            | MachineRequest::Snapshot
            | MachineRequest::Events { .. }
            | MachineRequest::Query { .. }
    )
}

pub(super) fn is_read_only_service_request(request: &MachineRequest) -> bool {
    matches!(
        request,
        MachineRequest::PrepareEffectBundle { .. }
            | MachineRequest::PrepareEffectComparison { .. }
            | MachineRequest::PrepareProjectedRecord { .. }
            | MachineRequest::RuntimeInspect { .. }
            | MachineRequest::VerifyBundleArchive { .. }
            | MachineRequest::ReadBundleEntry { .. }
            | MachineRequest::Reconcile { .. }
    )
}
