# Accounts, models and task routing

The execution catalog belongs to Zap Wayfinder and is shared by its clients.
An account connection points to a protected login binding on the execution
host. A named configuration authorizes a model and its allowed effort/context
choices on that connection. Changing a name does not change the account.

For normal setup, use **Accounts, agents and task routing** in Quick Lens.
Enable only the configurations you want Zap to use. Reference entries and
their specialization scores are editable starting preferences, not measured
benchmarks or permission to launch a model.

Managed work selects a specialization explicitly. The shared resolver filters
unavailable or unauthorized choices before applying the economy/quality slider,
specialization scores and applicable fresh subscription readings. Preview and
dispatch use the same ranking rules. A manual selection has a recorded reason.
The run retains its chosen configuration, account, model, effort and context.
Changing the catalog affects future work; it does not move a conversation to
another account.

## Additional Codex or Claude accounts

The default bindings use the host's `CODEX_HOME` and `CLAUDE_CONFIG_DIR`, or the
agent's normal home when those variables are unset. Another account needs a
separate home and its own login. Create that directory and sign in through the
agent's ordinary UI in a temporary terminal environment. For example:

```powershell
$zapAccountHome = Join-Path $env:USERPROFILE '.vibe/zap/accounts/codex-secondary'
New-Item -ItemType Directory -Force -Path $zapAccountHome | Out-Null
$savedCodexHome = $env:CODEX_HOME
try {
    $env:CODEX_HOME = $zapAccountHome
    codex login
} finally {
    $env:CODEX_HOME = $savedCodexHome
}
```

For Claude Code, use a different directory, temporarily set
`CLAUDE_CONFIG_DIR`, and run `claude` to complete its normal sign-in. Restore the
previous environment variable afterwards. Do not copy another account's auth
file or replace a running agent's login.

Add a protected binding to `~/.vibe/zap/settings.json`. Merge the following
field into your existing settings; keep other profiles, proxy settings and
bindings. The example path must be replaced with the actual absolute path on
your machine:

```json
{
  "version": 1,
  "executionBindings": [
    {
      "bindingId": "binding.codex.secondary",
      "hostId": "host.execution.local",
      "displayName": "Codex secondary account",
      "enabled": true,
      "setupGuidance": "Sign in through Codex using this account's separate CODEX_HOME.",
      "kind": "codex_home",
      "agentProduct": "codex",
      "homePath": "C:/Users/YOUR_NAME/.vibe/zap/accounts/codex-secondary"
    }
  ]
}
```

A Claude home uses `kind: "claude_config_dir"` and
`agentProduct: "claude_code"`. Stop and restart Wayfinder to reload protected
host settings. The new binding then appears in the account selector. Add its
connection and model configurations in the UI. Protected bindings select login
isolation; browser commands cannot supply arbitrary paths or executables.

OpenCode, Qwen Code and API-based profiles may use a protected
`environment_reference` binding with an `environmentRef` that resolves through
the host's `environmentFiles` settings. These are existing provider-profile
integrations. Endpoint and credential setup stays with that protected profile,
not the model-priority table.

## Limits and provider differences

Reasoning effort is available only where the selected adapter can apply it.
Native children may inherit parent settings; the UI must not promise an
independent setting where the host supplies none. Context choices distinguish
a model's documented maximum from the limit actually applied by the host.

Quota readings retain their source, observation time, window and reset time.
The threshold uses selected applicable subscription meters. Unknown readings
do not penalize an account. A low reading lowers priority and does not revoke
an otherwise allowed configuration. No reset, credit purchase or model polling
is part of quota refresh.

The reference table covers model families, while this release has four live
agent integrations. A family listed in the table still needs a compatible,
configured execution path. Image output additionally requires an available
image-generation tool; selecting an image model as a coding conversation model
is refused.

ZapMock account bindings are explicitly synthetic, configured by the trusted
test host and separate from normal account discovery. They run the
`zap-mock/deterministic-v1` algorithm and never use an LLM. Synthetic model
personas are evidence for routing and protocol behavior, not provider quality.
