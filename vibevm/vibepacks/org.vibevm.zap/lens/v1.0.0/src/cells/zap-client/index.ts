/**
 * Trusted Node-side ZAP HTTP client. Do not export this cell from renderer barrels.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-002#plan-control
 */
export {
  canonicalQueryInput,
  createZapClient,
  protectedCommandDigest,
  type ZapClientOptions,
} from "./client.ts";
export {
  encodeCanonicalJson,
  parseCanonicalJson,
  parseWireJson,
  rustSerdeJsonValue,
} from "./codec.ts";
export {
  ActiveContextViewSchema,
  AdvanceChangeAdmissionInputSchema,
  ChangeAdmissionViewSchema,
  LosslessJsonSchema,
  U32WireSchema,
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
  ProtectedSubmissionSchema,
  PrepareBundleInputSchema,
  PrepareCompositeSuccessorInputSchema,
  PrepareComparisonInputSchema,
  PreparedComparisonSchema,
  PreparedCompositeSuccessorSchema,
  PreparedCompositeSuccessorWireSchema,
  RecordedCompositeSuccessorWireSchema,
  StoredPreparedComparisonSchema,
  PrepareProjectedInputSchema,
} from "./schemas.ts";
export type * from "./types.ts";
