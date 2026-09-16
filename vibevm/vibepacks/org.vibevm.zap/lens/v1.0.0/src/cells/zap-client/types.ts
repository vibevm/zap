import type { $brand, ZodType } from "zod";

/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
export type LosslessNumberText = string & { readonly __lossless_number: unique symbol };
export type U64 = LosslessNumberText & { readonly __u64: unique symbol };
export type ZapId = string & $brand<"ZapId">;
export type ZapDigest = string & $brand<"ZapDigest">;

export type CanonicalJsonInput =
  | null
  | boolean
  | string
  | number
  | bigint
  | readonly CanonicalJsonInput[]
  | { readonly [key: string]: CanonicalJsonInput };

export type LosslessJsonValue =
  | null
  | boolean
  | string
  | LosslessNumberText
  | readonly LosslessJsonValue[]
  | { readonly [key: string]: LosslessJsonValue };

export interface ZapCredential {
  readonly id: ZapId;
  readonly bearer: string;
}

export interface ZapExchangeRequest {
  readonly method: "GET" | "POST";
  readonly url: URL;
  readonly headers: Readonly<Record<string, string>>;
  readonly body?: Uint8Array;
  readonly signal?: AbortSignal;
}

export interface ZapExchangeResponse {
  readonly status: number;
  readonly headers: Readonly<Record<string, string>>;
  readonly body: Uint8Array;
}

export interface ZapHttpExchange {
  request(input: ZapExchangeRequest): Promise<ZapExchangeResponse>;
}

export interface ZapStoreIdentity {
  readonly store_id: ZapId;
  readonly campaign_id: ZapId;
  readonly base_id: ZapId;
  readonly store_epoch: string;
  readonly codec_epoch: number;
  readonly reducer_epoch: number;
}

export interface ZapRefusal {
  readonly code: string;
  readonly requirement: string;
  readonly message: string;
  readonly fix: string;
  readonly detail: ZapErrorDetail;
}

export type ZapErrorDetail =
  | { readonly kind: "none" }
  | { readonly kind: "invalid_identity"; readonly identity_type: string; readonly byte_len: U64 }
  | { readonly kind: "stale_revision"; readonly expected: U64; readonly actual: U64 }
  | { readonly kind: "conflicting_ids"; readonly command_id: ZapId; readonly event_id: ZapId }
  | {
      readonly kind: "missing_subjects";
      readonly subjects: readonly { readonly kind: string; readonly id: ZapId }[];
    }
  | {
      readonly kind: "unsupported_epoch";
      readonly family: string;
      readonly requested: number;
      readonly supported: readonly number[];
    }
  | {
      readonly kind: "violated_limit";
      readonly name: string;
      readonly maximum: U64;
      readonly actual: U64;
    }
  | { readonly kind: "active_holds"; readonly holds: readonly ZapId[] }
  | { readonly kind: "active_pauses"; readonly pauses: readonly ZapId[] }
  | { readonly kind: "pending_effects"; readonly jobs: readonly ZapId[] };

/** @implements spec://org.vibevm.zap/lens/PROP-002#plan-control */
export type ZapClientError =
  | { readonly kind: "configuration"; readonly message: string }
  | { readonly kind: "transport"; readonly message: string }
  | { readonly kind: "http_refusal"; readonly status: number; readonly refusal: ZapRefusal }
  | { readonly kind: "resync_required"; readonly reason: string }
  | { readonly kind: "malformed_response"; readonly message: string }
  | {
      readonly kind: "foreign_response";
      readonly expected: string;
      readonly received: string;
    }
  | { readonly kind: "limit_exceeded"; readonly maximum: number; readonly actual: number }
  | { readonly kind: "stale_context"; readonly message: string }
  | {
      readonly kind: "uncertain_submission";
      readonly command_id: ZapId;
      readonly command_digest: ZapDigest;
      readonly reconcile_required: true;
    }
  | {
      readonly kind: "uncertain_operation";
      readonly operation_id: ZapId;
      readonly route: "/v1/change/admission";
      readonly retry_exact: true;
    };

export type ZapClientResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: ZapClientError };

export interface ZapCapabilities {
  readonly schema: string;
  readonly read_operations: readonly string[];
  readonly command_operations: readonly string[];
  readonly query_ids: readonly ZapId[];
  readonly unavailable_operations: readonly string[];
  readonly max_page_items: number;
  readonly local_bind_default: boolean;
}

export interface ZapSnapshot {
  readonly store: ZapStoreIdentity;
  readonly revision: U64;
  readonly head_event_digest: ZapDigest;
  readonly event_count: U64;
  readonly record_count: U64;
  readonly index_count: U64;
  readonly projection_digest: ZapDigest;
  readonly physical_schema_version: number;
  readonly physical_schema: "v1" | "v2";
  readonly physical_projection_algorithm: "v1_tables" | "v2_tables";
  readonly physical_projection_digest: ZapDigest;
  readonly logical_row_digest: ZapDigest;
  readonly derived_index_catalog: ZapIndexCatalog | null;
}

export interface ZapIndexCatalog {
  readonly version: number;
  readonly query_epoch: number;
  readonly covered_revision: U64;
  readonly families: readonly ZapId[];
  readonly algorithms: readonly {
    readonly family: ZapId;
    readonly fingerprint: ZapDigest;
  }[];
}

export interface ZapQueryPage<T> {
  readonly store: ZapStoreIdentity;
  readonly revision: U64;
  readonly query_epoch: number;
  readonly items: readonly T[];
  readonly completeness: "complete" | "more" | "unknown_boundary";
}

export interface ActiveContextSnapshot {
  readonly store_id: ZapId;
  readonly campaign_id: ZapId;
  readonly base_id: ZapId;
  readonly revision: U64;
}

export type ActiveOutcomeRef =
  | { readonly state: "uninitialized" }
  | { readonly state: "present"; readonly outcome_id: ZapId; readonly record_revision: U64 };

export type CurrentStrategyRef =
  | { readonly state: "absent" }
  | {
      readonly state: "present";
      readonly strategic_revision_id: ZapId;
      readonly record_revision: U64;
    };

export interface MilestonePlanKey {
  readonly outcome_id: ZapId;
  readonly generation: U64;
}

export type AdoptedPlanGap =
  | "plan_record_missing"
  | "plan_state_mismatch"
  | "outcome_binding_stale"
  | "strategy_record_missing"
  | "strategy_binding_stale"
  | "current_strategy_mismatch";

export type AdoptedMilestonePlanRef =
  | { readonly state: "absent" }
  | {
      readonly state: "present";
      readonly plan_key: MilestonePlanKey;
      readonly plan_state_revision: U64;
    }
  | {
      readonly state: "needs_reassessment";
      readonly plan_key: MilestonePlanKey;
      readonly plan_state_revision: U64;
      readonly gaps: readonly AdoptedPlanGap[];
    };

export interface ActiveContextView {
  readonly snapshot: ActiveContextSnapshot;
  readonly active_outcome: ActiveOutcomeRef;
  readonly current_strategy: CurrentStrategyRef;
  readonly adopted_milestone_plan: AdoptedMilestonePlanRef;
}

export interface EventCursor {
  readonly store: ZapStoreIdentity;
  readonly revision: U64;
  readonly next_sequence: U64;
}

export interface ZapEventSummary {
  readonly sequence: U64;
  readonly digest: ZapDigest;
  readonly header: LosslessJsonValue | null;
  readonly reason: LosslessJsonValue | null;
  readonly authority: LosslessJsonValue | null;
  readonly action_admission: LosslessJsonValue | null;
  readonly artifacts: readonly ZapDigest[];
}

export interface ZapEventPage {
  readonly store: ZapStoreIdentity;
  readonly revision: U64;
  readonly events: readonly ZapEventSummary[];
  readonly resume: EventCursor;
  readonly next: EventCursor | null;
}

export interface ZapStreamPage {
  readonly events: readonly ZapEventSummary[];
  readonly cursor: EventCursor;
}

export type PreparationRead =
  | { readonly kind: "current" }
  | { readonly kind: "revision"; readonly revision: bigint };

export interface QueryInputWire {
  readonly codec: 2;
  readonly canonical_json: readonly number[];
}

export interface EffectDraftInput {
  readonly effect_id: ZapId;
  readonly index: number;
  readonly kind: ZapId;
  readonly payload: QueryInputWire;
  readonly predecessors: readonly ZapId[];
  readonly product_event_id: ZapId;
}

export interface EffectBundleDraftInput {
  readonly alternative_id: ZapId;
  readonly committed_prefix: readonly ZapId[];
  readonly effects: readonly EffectDraftInput[];
  readonly no_op_basis: CanonicalJsonInput | null;
}

export interface PrepareBundleRequest {
  readonly at: PreparationRead;
  readonly actor: CanonicalJsonInput | null;
  readonly draft: EffectBundleDraftInput;
}

export interface PrepareComparisonRequest {
  readonly at: PreparationRead;
  readonly actor: CanonicalJsonInput | null;
  readonly draft: {
    readonly assessment_id: ZapId;
    readonly alternatives: readonly EffectBundleDraftInput[];
    readonly policy: CanonicalJsonInput;
    readonly capacity: CanonicalJsonInput;
    readonly closure: CanonicalJsonInput;
  };
}

export interface PrepareProjectedRecordRequest extends PrepareBundleRequest {
  readonly record: { readonly family: ZapId; readonly key: readonly number[] };
}

export interface PrepareCompositeSuccessorRequest {
  readonly operation_id: ZapId;
  readonly store: ZapStoreIdentity;
  readonly expected_revision: bigint;
  readonly precursors: EffectBundleDraftInput;
  readonly plan_intent: QueryInputWire;
}

export interface PreparedEffectBundleView {
  readonly store: ZapStoreIdentity;
  readonly observed_revision: U64;
  readonly request: LosslessJsonValue;
  readonly preflight: LosslessJsonValue;
  readonly affected_scopes: readonly (LosslessJsonValue | null)[];
}

export interface PreparedEffectComparisonView {
  readonly store: ZapStoreIdentity;
  readonly observed_revision: U64;
  readonly alternatives: readonly PreparedEffectBundleView[];
  readonly basis_request: LosslessJsonValue;
  readonly relevant_basis: ZapDigest;
  readonly affected_scopes: readonly AffectedScopeView[];
}

export interface PreparedCompositeSuccessorView {
  readonly request: Omit<PrepareCompositeSuccessorRequest, "expected_revision"> & {
    readonly expected_revision: U64;
  };
  readonly request_digest: ZapDigest;
  readonly plan: QueryInputWire;
  readonly precursor_preparation: PreparedEffectBundleView;
  readonly reconciliation: ReconcileRequest;
  /** Exact canonical backend value used to replay arbitrary numeric fields without JSON loss. */
  readonly replay: QueryInputWire;
}

export interface RecordedCompositeSuccessorView {
  readonly prepared: PreparedCompositeSuccessorView;
  readonly submission: SubmissionStatus;
}

export interface WorkExecutionObservationRecord {
  readonly job_id: ZapId;
  readonly attempt_id: ZapId;
  readonly work_id: ZapId;
  readonly contract_id: ZapId;
  readonly contract_digest: ZapDigest;
  readonly validation_generation: U64;
  readonly subjects: readonly { readonly kind: string; readonly id: ZapId }[];
  readonly execution:
    | "prepared"
    | "dispatch_pending"
    | "starting"
    | "running"
    | "stop_requested"
    | "stopping"
    | "succeeded"
    | "failed"
    | "stopped"
    | "interrupted"
    | "unknown_effect";
  readonly effect: "not_started" | "intent_committed" | "started" | "completed" | "unknown";
  readonly safe_state: "unknown" | "needs_reconcile" | "not_started" | "safe" | "completed";
  readonly revision: U64;
}

export interface AffectedScopeView {
  readonly request_digest: ZapDigest;
  readonly observed_revision: U64;
  readonly affected_work_ids: readonly ZapId[];
  readonly dependent_work_ids: readonly ZapId[];
  readonly subjects: readonly { readonly kind: string; readonly id: ZapId }[];
  readonly unknown_boundary: readonly { readonly kind: string; readonly id: ZapId }[];
  readonly completeness: "complete" | "incomplete";
  readonly relevant_basis: ZapDigest;
  readonly jobs: {
    readonly request_digest: ZapDigest;
    readonly observed_revision: U64;
    readonly jobs: readonly WorkExecutionObservationRecord[];
    readonly completeness: "complete" | "unknown";
  };
  readonly digest: ZapDigest;
}

export interface ProjectedRecordView {
  readonly store: ZapStoreIdentity;
  readonly observed_revision: U64;
  readonly family: ZapId;
  readonly key: Uint8Array;
  readonly canonical_value: Uint8Array | null;
  readonly preparation: PreparedEffectBundleView;
}

export interface ProtectedCommand {
  readonly frame: {
    readonly header: {
      readonly protocol: number;
      readonly store_id: ZapId;
      readonly campaign_id: ZapId;
      readonly base_id: ZapId;
      readonly command_id: ZapId;
      readonly event_id: ZapId;
      readonly expected_revision: bigint;
      readonly kind: ZapId;
      readonly causes: readonly ZapId[];
      readonly basis:
        | { readonly kind: "not_applicable" }
        | { readonly kind: "exact"; readonly digest: ZapDigest };
    };
    readonly reason: {
      readonly summary: string;
      readonly evidence: readonly ZapId[];
      readonly decision: ZapId | null;
      readonly change: ZapId | null;
    };
    readonly payload: QueryInputWire;
  };
}

export interface ReconcileRequest {
  readonly command_id: ZapId;
  readonly command_digest: ZapDigest;
}

export interface CommitReceiptView {
  readonly store: ZapStoreIdentity;
  readonly command_id: ZapId;
  readonly event_id: ZapId;
  readonly transaction_id: ZapId;
  readonly revision: U64;
  readonly event_digest: ZapDigest;
  readonly output: Uint8Array;
  readonly disposition: "committed" | "exact_retry" | "reconciled_committed";
}

export interface ProtectedSubmission {
  readonly command: ProtectedCommand;
  readonly reconciliation: ReconcileRequest;
}

export interface AdvanceChangeAdmissionRequest {
  readonly operation_id: ZapId;
  readonly store: ZapStoreIdentity;
  readonly expected_revision: bigint;
  readonly action: ZapId;
  readonly assessment_id: ZapId;
  readonly alternative_id: ZapId;
  readonly source_assessment_digest: ZapDigest;
  readonly assessment_digest: ZapDigest;
  readonly relevant_basis: ZapDigest;
  readonly comparison: PrepareComparisonRequest;
  readonly product: ProtectedCommand;
  readonly decision_id: ZapId | null;
  readonly exception_id: ZapId | null;
}

export type ChangeAdmissionView =
  | {
      readonly status: "ready";
      readonly operation_id: ZapId;
      readonly assessment_id: ZapId;
      readonly alternative_id: ZapId;
      readonly observed_revision: U64;
      readonly adjudication: LosslessJsonValue;
      readonly admission: LosslessJsonValue;
    }
  | {
      readonly status: "owner_decision_required";
      readonly operation_id: ZapId;
      readonly assessment_id: ZapId;
      readonly alternative_id: ZapId;
      readonly observed_revision: U64;
      readonly hold_id: ZapId;
      readonly assessment_digest: ZapDigest;
      readonly adjudication: LosslessJsonValue;
      readonly decision: {
        readonly assessment_digest: ZapDigest;
        readonly forecast_id: ZapId | null;
        readonly forecast_digest: ZapDigest | null;
        readonly policy_id: ZapId;
        readonly policy_revision: U64;
        readonly recommended_alternative_id: ZapId;
        readonly effect_fingerprints: readonly ZapDigest[];
        readonly effect_preflight_digests: readonly ZapDigest[];
        readonly decision_revision: U64;
      };
    };

export type ProtectedChannel = "command" | "control" | "observation" | "agent";

export type SubmissionStatus =
  | { readonly status: "committed"; readonly receipt: CommitReceiptView }
  | { readonly status: "not_committed"; readonly command_id: ZapId }
  | {
      readonly status: "unknown";
      readonly command_id: ZapId;
      readonly command_digest: ZapDigest;
    };

export interface ZapClient {
  capabilities(signal?: AbortSignal): Promise<ZapClientResult<ZapCapabilities>>;
  snapshot(signal?: AbortSignal): Promise<ZapClientResult<ZapSnapshot>>;
  events(
    after: EventCursor | null,
    limit: number,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapEventPage>>;
  streamEvents(
    after: EventCursor | null,
    limit: number,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapStreamPage>>;
  query<T>(
    queryId: ZapId,
    input: CanonicalJsonInput,
    itemSchema: ZodType<T>,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapQueryPage<T>>>;
  activeContext(signal?: AbortSignal): Promise<ZapClientResult<ZapQueryPage<ActiveContextView>>>;
  prepareBundle(
    request: PrepareBundleRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<PreparedEffectBundleView>>;
  prepareComparison(
    request: PrepareComparisonRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<PreparedEffectComparisonView>>;
  prepareProjectedRecord(
    request: PrepareProjectedRecordRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ProjectedRecordView>>;
  prepareCompositeSuccessor(
    request: PrepareCompositeSuccessorRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<PreparedCompositeSuccessorView>>;
  recordCompositeSuccessor(
    prepared: PreparedCompositeSuccessorView,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<RecordedCompositeSuccessorView>>;
  submit(
    channel: ProtectedChannel,
    submission: ProtectedSubmission,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<SubmissionStatus>>;
  reconcile(
    request: ReconcileRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<SubmissionStatus>>;
  advanceChangeAdmission(
    request: AdvanceChangeAdmissionRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ChangeAdmissionView>>;
}
