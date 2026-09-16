# ZAP CLI implementation API

Public behavior is maintained in `../../flows/zap/ZAP-CLI.md` and the machine
contract at `../../examples/zap/command-contract.json`.

`zaplib.application_cli.main(argv=None) -> int` is the public dispatcher.
`scripts/zap.py` calls it directly. `zap_state.py` retains its runpy helper
exports and calls the new main only when executed as a script.

Trust bootstrap writes `zap-trust/1` plus per-role token files outside the
campaign store. `load_trust` binds both campaign ID and base hash.
`build_automatic_coordinator(engine, profile_path)` resolves relative paths
against the profile directory and constructs the frozen E runtime surface.

The CLI always emits one JSON value except argparse help. Public `record` calls
`ApplicationService.submit_agent`. Control, action and observation commands
read opaque credentials from protected files; token values are never command
arguments.
