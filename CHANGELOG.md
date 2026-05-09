# Changelog

All notable changes to docket are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each `## [VERSION]` section is consumed verbatim by the release workflow as the
GitHub Release body for the matching tag — keep entries written for that audience.

## [Unreleased]

## [0.0.1] - 2026-05-09

Initial release. v1 CLI shell with embedded prompts and bundled SQLite store.

### Added

- CLI verbs: `init`, `add`, `ls`, `show`, `ready`, `blocked`, `status`, `done`, `rm`, `prompt`, `group {new,ls,show,close}`.
- Per-repo SQLite store at `.docket/db.sqlite` (auto-gitignored on `init`).
- Two tables: `tasks` (with first-class `acceptance` and `deps` fields) and `groups`.
- Computed `ready` queue and `blocked` debug view from unmet deps.
- Four prompt templates embedded at build time via `include_str!`: `tdd-pursuit`, `create-task`, `commit`, `pr`.
- `--json` output on every list/show command for agent consumption.
- Release pipeline: tag-triggered cross-builds for `{macOS, Linux} × {x86_64, aarch64}`, GitHub Release with checksums, `install.sh` for `curl | bash` install.

### Notes

- Versioning starts at `0.0.x` deliberately — `0.1.0` is reserved for when `docket start` lands and the loop is dogfood-proven.

[Unreleased]: https://github.com/parzival1l/docket/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/parzival1l/docket/releases/tag/v0.0.1
