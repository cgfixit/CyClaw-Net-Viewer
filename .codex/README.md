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

## Optimize NetViewer

Three additional repository-discoverable skills are maintained in
`../.agents/skills`: `emulate-egress-sandbox` for Bash traffic observation,
`verify-netviewer-rust` for compiler compatibility, and
`verify-netviewer-bundle` for packaging verification. Invoke them with their
`$skill-name`. They use the repository's scripts and tests from its root;
copying a skill to a personal installation does not copy the application.

`skills/optimize-netviewer/SKILL.md` contains the repository-specific improvement
workflow and `agents/openai.yaml` supplies its Codex UI metadata. Invoke the
installed skill with `$optimize-netviewer` or select it through `/skills`.
The shared source stays under `.codex/skills` as a portable installable copy;
for current automatic repository discovery, Codex documents `.agents/skills`.
Install/copy the skill into your personal skill location rather than changing
`CODEX_HOME` to this repository.

For the requested legacy slash entry point, copy
`skills/optimize-netviewer/assets/optimize-netviewer.md` into your personal
Codex home's `prompts/optimize-netviewer.md`, then restart the CLI/IDE session.
Invoke `/prompts:optimize-netviewer` with optional focus text. Custom prompts
are deprecated; `$optimize-netviewer` is the skill entry point. See the official
[skill documentation](https://learn.chatgpt.com/docs/build-skills) and
[custom-prompt documentation](https://learn.chatgpt.com/docs/custom-prompts).
