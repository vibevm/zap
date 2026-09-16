/** Exact authenticated Zap MCP tools eligible for local preauthorization. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
export const ZAP_MCP_SERVER_NAME = "zap-wayfinder";

const COMMUNICATION_TOOLS = [
  "codlens_assigned_context",
  "codlens_context",
  "codlens_emit",
  "codlens_finish",
  "codlens_ask_user_question",
  "codlens_inbox",
  "codlens_inbox_wait",
  "codlens_ack",
  "codlens_managed_work_profiles",
  "codlens_managed_work_read",
  "codlens_managed_work_report",
  "codlens_managed_work_attachment_ack",
  "codlens_native_work_before",
  "codlens_native_work_read",
  "codlens_native_work_attachment_ack",
] as const;

const DELEGATION_TOOLS = [
  "codlens_delegate",
  "codlens_forward",
  "codlens_managed_work_create",
  "codlens_managed_work_start",
] as const;

export function zapPreauthorizedToolNames(allowDelegation: boolean): readonly string[] {
  return allowDelegation ? [...COMMUNICATION_TOOLS, ...DELEGATION_TOOLS] : COMMUNICATION_TOOLS;
}
