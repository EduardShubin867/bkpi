# BKPI

Local installer for the BKPI Rust runtime and coding-agent integrations.

**KPI + plan + Bitrix data → agent assessment → deterministic calculation in Rust.**

Requires Node.js 22+. Rust is not required on the user's machine.

## Install

```sh
npx @eduard-shubin/bkpi
```

The installer downloads the native BKPI runtime for the current platform from the GitHub Release, verifies its SHA256 checksum and guides you through Bitrix24 and agent setup.

Supported platforms:

- macOS Apple Silicon
- macOS Intel
- Linux x64
- Linux ARM64
- Windows x64

Useful commands:

```sh
npx @eduard-shubin/bkpi doctor
npx @eduard-shubin/bkpi targets
npx @eduard-shubin/bkpi@latest update
npx @eduard-shubin/bkpi uninstall
```

After installation, the native runtime is available as the regular `bkpi` command.

## Setup

The setup wizard can:

- add or update Bitrix24 integrations;
- choose local file, OS keychain or environment-variable credential storage;
- configure Codex and/or Claude Code;
- install the shared BKPI skill and MCP registration.

Webhook input is masked and passed to the native runtime through stdin. Credentials are not placed in shell argv.

The default credential backend is a private local file outside the workspace. OS keychain and environment variables are explicit alternatives.

## Workspace

Run initialization inside the project where you want to use BKPI:

```sh
bkpi init
```

Choose the integration, people and optional KPI documents. Then open a new Codex or Claude Code session in that project and ask the agent to analyze the KPI.

Examples:

```text
Посмотри, как у меня идёт KPI за сентябрь.
Разбери KPI Ивана подробно.
По каким KPI у команды не хватает evidence?
```

## Updating

`update` installs the runtime matching the version of the npm installer that launched it.

To upgrade to the newest release:

```sh
npx @eduard-shubin/bkpi@latest update
```

Then refresh agent integrations if needed:

```sh
npx @eduard-shubin/bkpi@latest targets
```

## Agent targets

BKPI supports Codex and Claude Code through their official CLIs.

```sh
npx @eduard-shubin/bkpi targets
```

The installer registers the BKPI MCP server and installs the shared skill. Existing unrelated agent configuration is preserved.

## Development

```sh
npm ci
npm run typecheck
npm run lint
npm run build
npm test
npm pack --dry-run
```

For local installer testing with a locally built runtime:

```sh
BKPI_RUNTIME_SOURCE=/absolute/path/to/bkpi node dist/index.js update
```

`BKPI_INSTALL_DIR` can be used to isolate the runtime installation directory during tests.

## Security

- release binaries are verified with SHA256 before installation;
- webhook values are not passed through argv;
- credentials live outside the workspace;
- Unix credential files use private permissions;
- local state writes are atomic;
- installer failures redact credential material from captured output.

## License

MIT
