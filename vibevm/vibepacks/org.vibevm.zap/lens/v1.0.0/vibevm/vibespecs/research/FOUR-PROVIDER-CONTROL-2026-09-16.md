# Four-provider control evidence — 2026-09-16

This records the mechanism selection for PROP-012. A supported CLI option or
protocol method is a design input; real pause/wake acceptance is recorded after
the implementation runs. No capability is established by a guessed terminal
prompt or by a successful write alone.

| Provider | Structured coordinator | Owned interactive worker |
|---|---|---|
| Codex 0.152.1 | App-server turn control and observed completion; retained thread continuation | Real TUI attached to an owned WebSocket app-server, with the same thread controlled through the protocol |
| Claude Code 2.1.220 | Stream JSON control initialization, interruption and observed results | Real TUI plus isolated authenticated hook observations; terminal input remains fenced by the human/automation lease |
| OpenCode 1.18.25 | Owned authenticated local server, session-scoped abort, events and prompt submission | Actual `attach` TUI against the owned server and exact session |
| Qwen Code 0.23.4 | Stream JSON control initialization and interruption | Actual TUI with a structured JSON output file, remote-input JSONL file and verified lifecycle observations |

The [Codex app-server reference](https://learn.chatgpt.com/docs/app-server)
distinguishes requesting interruption from observed turn completion. Its resume
operation reopens a saved thread for subsequent turns. Installed Codex help also
confirms TUI `--remote`, `--remote-auth-token-env`, WebSocket app-server listening
and capability-token configuration. Only an owned loopback listener is needed.

The [Claude hook reference](https://code.claude.com/docs/en/hooks) describes
per-turn, permission and session lifecycle hooks. A hook firing while the provider
is deciding whether to continue is not automatically proof that it accepts new
input. The implementation must use the installed event semantics and its actual
generated configuration. Installed help confirms `--settings` and
`--setting-sources`; `--bare` disables OAuth/keychain lookup, so it cannot be
silently imposed on an account configured through ordinary login.

The [OpenCode server reference](https://opencode.ai/docs/server/) supplies the
session API and event stream. Installed `attach --help` confirms exact session
and directory selection and authentication through the server password/username
environment. Existing owned-server composition can be shared without turning
the terminal into a reconstructed headless transcript.

Installed Qwen help explicitly describes `--json-file` as structured dual output
while the TUI continues rendering, and `--input-file` as a remote JSONL command
channel. This avoids extra file-descriptor assumptions in Windows pseudoterminals.
The [Qwen hook documentation](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/)
is additional lifecycle evidence. Its bare mode and generated configuration must
be checked together so required observations are not disabled accidentally.

Common rules remain provider-independent: pause fences dispatch first; observed
idle settles it; background notices remain queued while busy, paused, stopped or
human-controlled. Continue preserves the conversation and does not replay the
coordinator boot. Transport correlation is explicit when a provider does not
supply a native turn identifier.
