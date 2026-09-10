# BKPI installer

Node 22+ is required, Rust is not. The npm package is prepared but not published.
Package name lives in `package.json.name`; rename it there if `bkpi` is unavailable.
Set `package.json.bkpi.releaseRepository` before publication, or use
`BKPI_RELEASE_REPOSITORY=owner/repo`. No repository owner is guessed.

Local development:

```sh
npm ci
npm run typecheck
npm run lint
npm run build
npm test
BKPI_RUNTIME_SOURCE=/absolute/path/to/bkpi node dist/index.js update
node dist/index.js setup
npm pack
npx --yes --package ./bkpi-0.6.0.tgz bkpi --help
```

`BKPI_INSTALL_DIR` overrides the user-local installation directory for isolated tests.
`BKPI_RUNTIME_SOURCE` explicitly supplies a locally built binary instead of a release download.
It must report the same version as this package. Never use this override with an untrusted binary.

Commands: `setup`, `doctor`, `update`, `uninstall`, `targets`, `init`,
`integration list/add/remove/doctor`, `config migrate`, `person kpi set/show`.
`update` installs the runtime matching this installer version; to upgrade releases use
`npx bkpi@latest update` once published. Then run `targets` to refresh shared agent skills.
No postinstall hook downloads or executes anything.

The wizard defaults to Private local file, with OS secure store and environment variables as explicit alternatives. It reads the webhook with a masked prompt and passes it to the Rust runtime through stdin only. Credential operations capture both output streams: failure details are suppressed, and successful output is redacted. The direct Rust CLI also supports masked input. Unix credential files use 0600; Windows uses user-directory ACLs. Environment setup asks only for origin and variable name and reports pending availability honestly.

Updates preserve the existing backend. Choose Change credential storage to run a verified move. Environment moves require a present variable and explicit confirmation; the previous secret is retained for recovery. See the root README for paths, permission repair and migration commands.

Codex and Claude adapters use their installed official CLIs. If a target CLI is absent,
install it first or choose CLI only. Existing unrelated config is preserved. Unmanaged
`bkpi` skill/MCP conflicts are reported, not overwritten. Managed targets can be removed.
Uninstall retains credentials and all workspace data; remove integrations separately.

`npm run build` generates Codex and Claude plugin bundles from `../agent/skill` under
`bundles/`. The default installer uses standalone skills plus user MCP registration;
these work across projects and use an absolute runtime executable. Plugin bundles are
an alternative distribution form and require `bkpi` on PATH. Do not install both forms
at once, as the same skill/server may appear twice.

The command runner uses [cross-spawn](https://github.com/moxystudio/node-cross-spawn)
for Windows npm command shims and argument escaping. Credentials never enter shell argv.

Setup asks for a hidden webhook and optional display name; the runtime generates
an internal ID. Updating a webhook is a separate named integration selection.
Run `init` in your project: choose people, supply optional KPI paths, and accept
copying (default Yes) or retain a reference. New documents are visible at `KPI.md`
or `kpi/<person-slug>.md`. The final summary prints absolute paths and chat fallback
instructions. Existing hidden KPI references remain supported.
