/**
 * Principal, actor, delegation and binding-generation transitions.
 * @scope spec://org.vibevm.zap/lens/PROP-001#identity
 */
import type { z } from "zod";

import {
  ActorDescriptorSchema,
  ActorIdSchema,
  BindingAuthSchema,
  BindingIdSchema,
  CapabilitySchema,
  ConnectInputSchema,
  ConnectionSchema,
  DelegateInputSchema,
  EnrollPrincipalInputSchema,
  ExpireActorInputSchema,
  PrincipalEnrollmentSchema,
  PrincipalIdSchema,
  ResumeInputSchema,
  type ActorDescriptor,
  type ActorId,
  type BindingAuth,
  type Capability,
  type ConnectInput,
  type Connection,
  type DelegateInput,
  type EnrollPrincipalInput,
  type ExpireActorInput,
  type PrincipalEnrollment,
  type PrincipalKind,
  type Result,
  type ResumeInput,
} from "../protocol/index.ts";
import { BrokerCore, credentialHash, fail, ok, type PrincipalContext } from "./core.ts";

const ROLE_CAPABILITIES: Readonly<Record<PrincipalKind, readonly Capability[]>> = {
  agent: [
    "message:emit",
    "question:ask",
    "question:cancel",
    "inbox:read",
    "inbox:ack",
    "inbox:forward",
    "actor:delegate",
    "actor:expire",
    "plan:propose",
  ],
  viewer: ["events:read"],
  human_responder: ["events:read", "question:answer", "question:amend"],
  human_plan_approver: ["events:read", "plan:approve"],
  trusted_execution_adapter: ["events:read", "plan:execute"],
};

export class IdentityOperations extends BrokerCore {
  enrollPrincipal(input: EnrollPrincipalInput): Result<PrincipalEnrollment> {
    return this.safe(() => {
      const parsed = EnrollPrincipalInputSchema.parse(input);
      if (
        !parsed.capabilities.every((capability) =>
          ROLE_CAPABILITIES[parsed.kind].includes(capability),
        )
      ) {
        return fail(
          "forbidden",
          "authority",
          "principal requested a capability outside its role",
          "enroll only capabilities permitted for the selected principal kind",
        );
      }
      return this.database.transaction(() => {
        const principalId = PrincipalIdSchema.parse(this.id("prn"));
        const principalToken = this.credential();
        this.database.run(
          `INSERT INTO principals
             (principal_id, kind, token_hash, workspace_ids_json, conversation_ids_json,
              capabilities_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)`,
          [
            principalId,
            parsed.kind,
            credentialHash(principalToken),
            JSON.stringify(parsed.workspaceIds),
            JSON.stringify(parsed.conversationIds),
            JSON.stringify(parsed.capabilities),
            this.now(),
          ],
        );
        return ok(PrincipalEnrollmentSchema.parse({ principalId, principalToken }));
      });
    });
  }

  connect(input: ConnectInput): Result<Connection> {
    return this.safe(() => {
      const parsed = ConnectInputSchema.parse(input);
      const principal = this.principal({ principalToken: parsed.principalToken });
      if (!principal.ok) return principal;
      const scopeError = this.requireScope(
        principal.value,
        parsed.workspaceId,
        parsed.conversationId,
      );
      if (scopeError !== null) return scopeError;
      if (!this.subset(parsed.capabilities, principal.value.capabilities)) {
        return fail(
          "forbidden",
          "identity",
          "actor capabilities exceed the principal scope",
          "request only a subset of the principal capabilities",
        );
      }
      return this.idempotent(
        principal.value.principalId,
        "new",
        parsed.clientRequestId,
        "connect",
        parsed,
        ConnectionSchema,
        () =>
          ok(
            this.createActor(
              principal.value,
              null,
              parsed.workspaceId,
              parsed.conversationId,
              parsed.capabilities,
              parsed.host,
              parsed.replyPolicy,
            ),
          ),
      );
    });
  }

  resume(input: ResumeInput): Result<Connection> {
    return this.safe(() => {
      const parsed = ResumeInputSchema.parse(input);
      const principal = this.principal({ principalToken: parsed.principalToken });
      if (!principal.ok) return principal;
      return this.idempotent(
        principal.value.principalId,
        parsed.actorId,
        parsed.clientRequestId,
        "resume",
        parsed,
        ConnectionSchema,
        () => {
          const actor = this.actor(parsed.actorId);
          if (
            actor === null ||
            actor.principalId !== principal.value.principalId ||
            actor.resumeTokenHash !== credentialHash(parsed.resumeCredential)
          ) {
            return fail(
              "unauthorized",
              "identity",
              "actor resume proof is invalid",
              "use the protected resume credential issued for this actor",
            );
          }
          const scopeError = this.requireScope(
            principal.value,
            actor.workspaceId,
            actor.conversationId,
          );
          if (scopeError !== null) return scopeError;
          const generation = actor.currentGeneration + 1n;
          const bindingId = BindingIdSchema.parse(this.id("bnd"));
          const bindingToken = this.credential();
          const now = this.now();
          this.database.run(
            `UPDATE bindings SET active = 0, revoked_at = ?
              WHERE actor_id = ? AND active = 1`,
            [now, actor.actorId],
          );
          const [kind, sessionId, subagentId, provenance] = this.hostValues(parsed.host);
          this.database.run(
            `UPDATE actors SET current_generation = ?, state = 'active', host_kind = ?,
                    host_session_id = ?, host_subagent_id = ?, host_provenance = ?, updated_at = ?
              WHERE actor_id = ?`,
            [generation, kind, sessionId, subagentId, provenance, now, actor.actorId],
          );
          this.database.run(
            `INSERT INTO bindings
               (binding_id, actor_id, principal_id, generation, token_hash, active, created_at)
             VALUES (?, ?, ?, ?, ?, 1, ?)`,
            [
              bindingId,
              actor.actorId,
              actor.principalId,
              generation,
              credentialHash(bindingToken),
              now,
            ],
          );
          const refreshed = this.actor(actor.actorId);
          if (refreshed === null) {
            return fail(
              "not_found",
              "identity",
              "resumed actor disappeared",
              "inspect the actor transaction",
            );
          }
          return ok(this.connection(refreshed, bindingId, bindingToken, parsed.resumeCredential));
        },
      );
    });
  }

  delegate(auth: BindingAuth, input: DelegateInput): Result<Connection> {
    return this.safe(() => {
      const parsedAuth = BindingAuthSchema.parse(auth);
      const parsed = DelegateInputSchema.parse(input);
      const parent = this.binding(parsedAuth);
      if (!parent.ok) return parent;
      const denied = this.requireCapability(parent.value, "actor:delegate");
      if (denied !== null) return denied;
      if (parent.value.state !== "active") return this.expiredActorFailure();
      if (!this.subset(parsed.capabilities, parent.value.capabilities)) {
        return fail(
          "forbidden",
          "identity",
          "delegated capabilities are not a subset of the parent",
          "remove capabilities the parent does not hold",
        );
      }
      const principal = this.principal({ principalToken: parsedAuth.principalToken });
      if (!principal.ok) return principal;
      return this.idempotent(
        parent.value.principalId,
        parent.value.actorId,
        parsed.clientRequestId,
        "delegate",
        parsed,
        ConnectionSchema,
        () =>
          ok(
            this.createActor(
              principal.value,
              parent.value.actorId,
              parent.value.workspaceId,
              parent.value.conversationId,
              parsed.capabilities,
              parsed.host,
              parsed.replyPolicy,
            ),
          ),
      );
    });
  }

  expireActor(auth: BindingAuth, input: ExpireActorInput): Result<ActorDescriptor> {
    return this.safe(() => {
      const parsed = ExpireActorInputSchema.parse(input);
      const caller = this.binding(BindingAuthSchema.parse(auth));
      if (!caller.ok) return caller;
      const denied = this.requireCapability(caller.value, "actor:expire");
      if (denied !== null) return denied;
      const target = this.actor(parsed.actorId);
      if (target === null) {
        return fail(
          "not_found",
          "identity",
          "actor does not exist",
          "use a durable actor ID from connect or delegate",
        );
      }
      if (
        target.actorId !== caller.value.actorId &&
        target.parentActorId !== caller.value.actorId
      ) {
        return fail(
          "forbidden",
          "identity",
          "caller cannot expire this actor",
          "expire self or an immediate delegated child",
        );
      }
      return this.idempotent(
        caller.value.principalId,
        caller.value.actorId,
        parsed.clientRequestId,
        "expire_actor",
        parsed,
        ActorDescriptorSchema,
        () => {
          this.database.run(
            `UPDATE actors SET state = 'expired', updated_at = ? WHERE actor_id = ?`,
            [this.now(), target.actorId],
          );
          const updated = this.actor(target.actorId);
          return updated === null
            ? fail(
                "not_found",
                "identity",
                "expired actor disappeared",
                "inspect the actor transaction",
              )
            : ok(this.descriptor(updated));
        },
      );
    });
  }

  protected createActor(
    principal: PrincipalContext,
    parentActorId: ActorId | null,
    workspaceId: string,
    conversationId: string,
    capabilities: readonly Capability[],
    host: z.output<typeof ConnectInputSchema>["host"],
    replyPolicy: z.output<typeof ConnectInputSchema>["replyPolicy"],
  ): Connection {
    const actorId = ActorIdSchema.parse(this.id("act"));
    const bindingId = BindingIdSchema.parse(this.id("bnd"));
    const bindingToken = this.credential();
    const resumeCredential = this.credential();
    const now = this.now();
    const [kind, sessionId, subagentId, provenance] = this.hostValues(host);
    this.database.run(
      `INSERT INTO actors
         (actor_id, principal_id, workspace_id, conversation_id, parent_actor_id,
          current_generation, state, capabilities_json, host_kind, host_session_id,
          host_subagent_id, host_provenance, reply_policy_json, resume_token_hash,
          created_at, updated_at) VALUES (?, ?, ?, ?, ?, 1, 'active', ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        actorId,
        principal.principalId,
        workspaceId,
        conversationId,
        parentActorId,
        JSON.stringify(capabilities),
        kind,
        sessionId,
        subagentId,
        provenance,
        JSON.stringify(replyPolicy),
        credentialHash(resumeCredential),
        now,
        now,
      ],
    );
    this.database.run(
      `INSERT INTO bindings
         (binding_id, actor_id, principal_id, generation, token_hash, active, created_at)
       VALUES (?, ?, ?, 1, ?, 1, ?)`,
      [bindingId, actorId, principal.principalId, credentialHash(bindingToken), now],
    );
    const row = this.actor(actorId);
    if (row === null) {
      throw new Error(
        "violates REQ spec://org.vibevm.zap/lens/PROP-001#identity: created actor disappeared; fix surface: inspect the actor transaction",
      );
    }
    return this.connection(row, bindingId, bindingToken, resumeCredential);
  }

  protected expiredActorFailure(): Result<never> {
    return fail(
      "conflict",
      "identity",
      "expired actor cannot publish new work",
      "resume the durable actor before publishing or delegating",
    );
  }

  protected subset(requested: readonly Capability[], held: readonly Capability[]): boolean {
    return requested.every((capability) => held.includes(CapabilitySchema.parse(capability)));
  }
}
