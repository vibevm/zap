# Start using Zap

Zap brings projects, agent conversations, questions, managed workers and Git
workspaces into one interface. Zap Wayfinder runs locally and keeps the durable
state. Zap Quick Lens opens that state in a browser or Electron window.

This guide is for the local 0.1 preview. The release acceptance record lists the
tested provider versions and the remaining limits. Gamelens, IDE clients and
collaboration between several people or computers are future components.

With a Vibe version that supports global user applications, install and start
Zap with the short native commands:

```text
vibe install -g org.vibevm.zap/zap
zap-quicklens
```

The default command starts an independent application and returns to the
terminal. `zap-quicklens start` is the explicit equivalent;
`zap-quicklens stop` shuts down that exact state owner. During development use
`zap-quicklens log` for an attached ordinary log stream or
`zap-quicklens debug` for detailed safe request and composition diagnostics.

Update and removal use `vibe update -g org.vibevm.zap/zap` and
`vibe uninstall -g org.vibevm.zap/zap`. An older Vibe CLI must itself be
updated before it can read the new global application declaration.

## Start the application

For a Windows portable distribution, extract the complete archive to a writable
directory and run **Start Zap.cmd**. The archive includes Node and Electron;
agent applications and Git remain separate installations. **Start Zap Desktop.cmd**
opens the same workspace in Electron. Keep the Wayfinder launcher running while
using Zap. Closing a viewer does not stop its agents.

For an npm distribution, install Node 24 or later, then install the supplied
package archive in a directory of your choice:

```powershell
npm install ./org.vibevm.zap-lens-1.0.0.tgz
npx --no-install zap-quicklens
```

The legacy `zap-quick-lens` command remains available. Use
`npx --no-install zap-quicklens --electron` for the desktop viewer. These
commands consume no inference until you explicitly start an agent or send work
to one. The application opens a paired local browser session automatically.

Install and sign in to at least one supported agent application: Codex, Claude
Code, OpenCode or Qwen Code. Run its ordinary CLI once to verify the login. Zap
uses that application's protected account configuration. It does not create an
account or supply a subscription.

Codex and Claude Code default logins have automatic local discovery. OpenCode
and Qwen Code use a configured protected provider profile, including its endpoint
and environment binding; follow the operator guide before adding those accounts
to the catalog.

## Add an account and a project

1. On the empty workspace, open **Accounts, agents and task routing**.
2. Select an available protected account binding and choose **Add account
   connection**. This connects an existing host login to the catalog.
3. Choose that connection and a compatible model, then **Add configuration**.
   The generated human name can be edited. Review its agent, model, permitted
   reasoning effort and context before saving.
4. Enter an existing project directory, select the named configuration and
   choose **Add project**. Registration does not start inference.
5. Choose **Open without starting** to inspect the workspace, or **Start
   development** to launch its coordinator. The coordinator reads the selected
   project's own instructions and can use the Zap communication tools.

The model reference catalog is a source of starting preferences, not an
authorization to spend on every listed model. Only enabled configurations with
a usable host binding are eligible. A model's published context limit does not
make every context size configurable in an agent CLI.

Use the economy/quality slider to set routing preference. Each managed task has
an explicit specialization, such as backend, web UI, research or image
generation. Routing considers that specialization and the allowed
configurations. You can inspect the proposed choice before starting work.

For image tasks, the default preference is a `gpt-5.6-sol` agent with a verified
image-generation tool. The image tool's model is distinct from the agent's
conversation model. A connection without that tool is not eligible for image
output merely because its model appears in the reference table.

## Everyday work

- Select a project to focus its conversation, questions, work and history.
  **All projects** shows the shared map and cross-project event queue.
- Send an ordinary message to the coordinator from the project conversation.
  New specifications and changes to the plan remain explicit project events.
- Answer **ZapAskUserQuestion** groups in Questions. Choices, multiple answers,
  free text and answer history share the same server channel as the agents.
- Open an agent or managed task to see its assignment, selected execution
  configuration, worktree, output and available controls. A completed process
  is not automatically an accepted result; review remains separate.
- Use Pause to hold new work and the provider's supported interrupt behavior.
  Continue resumes through the recorded session. Stop ends the owned process;
  retained history and work files remain available. The UI shows controls that
  the selected adapter supports.
- Attach notes to map objects to preserve ideas for later. Deferred
  instructions are delivered at their declared work boundary. Trash retains
  archived notes and removed objects for review and recovery.
- In a Git project, Workspaces can prepare another top-level plan or an
  isolated worker worktree. Integration shows the candidate diff, configured
  check and review before promotion. Git must be installed, and commit-producing
  operations require your configured author identity.

The graph has two distinct readings. **Goal structure** places decomposition
outward from the known goal. **Work order** follows known prerequisites.
Coordinates do not promise a duration, readiness or order where the source
does not supply one.

## Subscription readings and several accounts

Enable quota deprioritization and set a remaining-percent threshold if you
want low subscription capacity to reduce an account's routing priority. Select
the observed subscription meter that applies to the configuration. An unknown
or stale reading remains unknown. Percentages are never converted into an
invented number of remaining tokens.

The Codex adapter can read the selected account's subscription buckets without
running a model. Other agents may report that the reading is unavailable.
API rate limits, account balances and consumed tokens are separate quantities.
Zap does not reset a subscription or purchase credits.

For another account of the same agent, use a separate protected account home
or environment binding. Renaming the same binding does not create another
account. Additional host bindings are advanced local setup; see the
[account setup guide](EXECUTION-CATALOG-GUIDE.md). Credentials, executable paths
and environment files are never entered
into the web catalog.

## State, updates and sharing

The default state directory is `~/.vibe/zap`. It contains local settings,
workspace databases and runtime ownership files. Agent credentials remain in
their protected agent homes or explicitly configured environment files.

For a separate evaluation workspace, launch with:

```text
zap-quicklens --state-dir ABSOLUTE_PATH_TO_NEW_STATE_DIRECTORY
```

For the portable package, pass the same argument to **Start Zap.cmd**. Use a
different `uiPort` in that directory's `settings.json` when running two local
owners simultaneously.

Before replacing the application files, stop its agents from the interface,
close the viewers and stop the Wayfinder launcher. Back up the complete state
directory while Wayfinder is stopped. Extract the new application separately
and point it at the retained state directory. Do not overwrite a running
installation.

Share the release archive, not your state directory, agent homes or paired
browser URL. Each recipient signs into their own agent applications and
creates their own local catalog. Ordinary startup listens on localhost.
Internet access requires the separate password-protected web profile and an
HTTPS tunnel described in the operator guide; sharing a release does not
publish your machine.

## Current integration boundary

Agent coordination and project/worktree management work in an ordinary local
workspace. Authoritative planning-engine operations require a configured Zap
planning source. A pending or unavailable source is shown explicitly; the UI
does not substitute demonstration data. Advanced source configuration and
reconnection are described in the operator guide.

Native subagents are controlled by their parent application. Managed workers
are processes owned by Zap. Their capabilities differ, and Zap does not turn
native execution into managed execution silently. Remote hosts, multiple
human identities and crowdsourced capacity are not part of this local preview.

See [the operator guide](WAYFINDER-GUIDE.md) for protected settings, proxies,
extra account bindings and planning-source configuration, and
[the acceptance record](research/ZAP-PRODUCT-ACCEPTANCE-2026-09-16.md) for the
evidence behind the supported flows.
