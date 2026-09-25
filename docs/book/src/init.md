# First run: `supercli init`

`supercli init` sets up a fresh workspace in one step:

1. Creates `~/.supercli` with mode `0700` (owner-only).
2. Writes a default, valid configuration (see the
   [config reference](config-reference.md)).
3. Seeds the built-in connector presets.
4. Starts the Host service.
5. Shows a pairing code and QR code for your first device.
6. Ends with `supercli doctor` — the init is only done when doctor is green.

```sh
supercli init
```

JSON output is available for scripting:

```sh
supercli init --json
```

To start over on a test machine, point `SUPERCLI_HOME` at an empty directory
instead of touching your real `~/.supercli`:

```sh
SUPERCLI_HOME=/tmp/supercli-test supercli init
```
