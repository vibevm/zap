use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tempfile::tempdir;
use zap_core::*;
use zap_domain::control::{WorkRecord, WorkRenamed, WorkRenamedSchema};
use zap_domain::economics::*;
use zap_domain::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use zap_domain::knowledge::{
    AdaptiveReviewRecord, ClosureAssessed, ClosureAssessedSchema, ClosureStatus,
    Feasibility as ReviewFeasibility, KnowledgeClosureRecord, KnowledgeEndpoint, ReviewAlternative,
    ReviewApplied, ReviewAppliedSchema, ReviewDecision, ReviewStatus, ReviewTransition,
    ReviewWorkChange, ReviewWorkOperation, SourceCaptureStatus, SourceKind, SourceRecord,
    SourceScope, SourceVersion, ValueAssessment,
};
use zap_domain::lowering::{
    LoweredNodeExecution, LoweredWorkBinding, LoweringRecord, PlanningRevisionState,
    ReviewReloweringKey, ReviewReloweringRecord, ReviewReloweringStatus, VerificationSelection,
};
use zap_domain::owner_control::*;
use zap_domain::seams::{
    CharterDutyAuthority, CompletionDutyDisposition, CompletionDutyPolicy, DomainMutation,
    LifecycleStatus, MaturityStage, ObligationDisposition, WorkKind, WorkState, WorkType,
};
use zap_store::RedbStore;
use zap_wire::*;

const SEED_KIND: &str = "test.economics-seed";
const PRODUCT_KIND: &str = "test.semantic-product";
const REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root";

include!("economics_service/seed_and_product.rs");
include!("economics_service/scan_guards.rs");
include!("economics_service/providers.rs");
include!("economics_service/fixtures.rs");
include!("economics_service/indexed_admission.rs");
include!("economics_service/whole_transaction.rs");
include!("economics_service/exception_consumption.rs");
include!("economics_service/independent_progress.rs");
include!("economics_service/pause_scope.rs");
include!("economics_service/owner_decision_tail.rs");
include!("economics_service/owner_decision.rs");
include!("economics_service/automatic_prefix.rs");
include!("economics_service/forecast_history.rs");
include!("economics_service/reducer_failure.rs");
include!("economics_service/rejection_release.rs");
include!("economics_service/review_sidecar.rs");
include!("economics_service/business_basis.rs");
