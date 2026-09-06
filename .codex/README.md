# Codex project setup

Open this repository as the project root. `../AGENTS.md` is the canonical
project instruction file (the `/init` starting point); keep it aligned with
the actual code and CI rather than regenerating it blindly.

- `config.toml`: shared workspace sandbox and approval defaults.
- `checklists/review.md`: manual pre-PR checklist; not an automatic hook.
- `../CONTRIBUTING.md`: setup, validation, and contribution commands.

Codex loads project configuration only for trusted projects. Review these
files before trusting a checkout. Model selection, credentials, MCP server
configuration, and trust decisions remain personal settings. Do not set
`CODEX_HOME` to this directory or store sessions and authentication here.
Dependency downloads and GitHub operations may need approved network access.

See the official [configuration documentation](https://learn.chatgpt.com/docs/config-file/config-basic)
and [AGENTS.md documentation](https://learn.chatgpt.com/docs/agent-configuration/agents-md).
