# Workspace interaction

`workspace-interaction` joins native host requests, shared rich questions, and
durable answer delivery without starting a model turn. Codex
`item/tool/requestUserInput` requests become ordinary `QuestionGroup` records;
the binding retains the exact coordinator session, process epoch, request ID,
thread, turn, and item. A response is journaled as in flight before it is sent
to app-server. A transport loss remains uncertain, and a response for an older
process epoch is refused.

Command, file, and permission approvals use `NativeApprovalRequest`. They are
separate from question answers and from ZAP Owner decisions. Permission
responses are currently presented as unavailable because their product-specific
response body is not safely mapped. Secret native input is not persisted as an
ordinary question.

Agent-authored rich questions use the MCP tool
`codlens_ask_user_question`, presented in instructions as
`/ZapAskUserQuestion`. Its body contains only a client request ID and the shared
rich question draft. The HTTP gateway first revalidates the current broker
binding, then `wayfinder-agent` resolves the registered broker
workspace/conversation to one trusted project/context. Answers are persisted to
the exact actor inbox; this proves broker persistence, not native child wake or
model consumption.
