# Contributing to STUIE

Thanks for your interest in improving STUIE. This document covers the
practical bits — how to set up a dev environment, run tests, structure a
change, and get it merged.

By participating in this project you agree to abide by the
[Code of Conduct](CODE_OF_CONDUCT.md).

If you find a security issue, **do not open a public issue**. Follow the
process in [SECURITY.md](SECURITY.md) instead.

---

## Quick links

- [Reporting bugs](#reporting-bugs)
- [Suggesting features](#suggesting-features)
- [Development setup](#development-setup)
- [Running tests](#running-tests)
- [Branch & commit conventions](#branch--commit-conventions)
- [Submitting a pull request](#submitting-a-pull-request)
- [Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco)
- [Coding guidelines](#coding-guidelines)

---

## Reporting bugs

Use the **Bug report** issue template. Please include:

- Provider type (`S3`, `GCS`, `Azure`) and authentication mode.
- Operating system and terminal emulator (icons rendering correctly
  depends on Nerd Font support).
- A redacted excerpt of `~/.config/stuie/config.json` — **strip all
  credentials** (`access_key_id`, `secret_access_key`,
  `credentials_path` contents, `project` IDs you consider sensitive).
- Steps to reproduce, expected vs. actual behaviour.
- The relevant log file from `logs/` if available.

## Suggesting features

Use the **Feature request** template. For larger changes (new provider,
new pane, config-schema change), open a discussion first so we can align
on shape before you write code. Small UX or quality-of-life improvements
can go straight to a PR.

---

## Development setup

### Prerequisites

- **Rust 1.85 or newer** — pinned via `rust-version` in `Cargo.toml`
  and verified by the `MSRV (1.85)` CI job. Stable channel is fine.

  See [MSRV bump policy](#msrv-bump-policy) below before raising it.
- **Cargo** (ships with Rust).
- A **Nerd Font** installed in your terminal so the file-type icons
  render correctly.
- For S3 integration tests: **Docker** (used to run a local MinIO).
- For GCS work: a Google Cloud project you can use for ad-hoc testing,
  or `gcloud auth application-default login`.

### Clone and build

```bash
git clone https://github.com/thmswt/stuie.git
cd stuie
cargo build
```

The first build pulls a non-trivial dependency tree (`object_store`,
`aws-sdk-s3`, `ratatui`); subsequent builds are cached.

### Recommended local hooks (optional but encouraged)

We use [lefthook](https://github.com/evilmartians/lefthook) for opt-in
pre-commit hooks. Lefthook is a single Go binary — unlike `pre-commit`
you do not need a Python toolchain.

The configured hooks (defined in `lefthook.yml`) are:

| Hook | What it does | When it runs |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | Only if `*.rs` files are staged |
| `clippy` | `cargo clippy --all-targets` (uses the lint policy in `Cargo.toml`) | Only if `*.rs` files are staged |
| `typos` | [`typos`](https://github.com/crate-ci/typos) on staged files, with the project allowlist in `_typos.toml` | Always; **skips silently if `typos` isn't installed** so the hook is gracefully optional |

Install:

```bash
# macOS
brew install lefthook
# Linux/other: https://lefthook.dev/installation/

# Optional but recommended — enables the typos hook
brew install typos-cli      # or: cargo install typos-cli

# Wire the hooks into .git/hooks/
lefthook install
```

Escape hatches when you need them:

- **Skip everything for a single commit:** `LEFTHOOK=0 git commit ...`
- **Skip one specific hook:** `LEFTHOOK_EXCLUDE=clippy git commit ...`
- **Per-developer overrides:** drop a `lefthook-local.yml` next to
  `lefthook.yml`. It's gitignored, and its fields merge over the
  shared config — handy if you want to *add* hooks (e.g., `cargo
  test`) on your own machine without forcing them on everyone.

CI runs the same `cargo fmt --check` and `cargo clippy` invocations
regardless, so skipping locally is safe — you just push and let the
checks fail there if anything was wrong.

---

## Running tests

The test suites are split by external dependency so the unit tests stay
fast and self-contained.

### Self-contained unit tests (no external services)

These run in CI and should pass on every machine:

```bash
cargo test --lib \
           --test config_tests \
           --test ui_utils_tests \
           --test app_tests \
           --test test_read_only_storage \
           --test test_gcs_scope
```

### S3 integration tests (require local MinIO)

`tests/test_s3_provider.rs` exercises the real S3 code path against a
local MinIO instance. The setup script seeds a deterministic dataset.

```bash
./tests/minio_setup.sh        # start MinIO + seed test data (Docker)
cargo test --test test_s3_provider
./tests/minio_teardown.sh     # cleanup
```

If MinIO isn't running you'll see connection-refused errors — that's
expected; just run `minio_setup.sh` first.

### Formatting and linting

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

CI runs both; please run them locally before pushing to avoid round-trips.

---

## Branch & commit conventions

### Branches

- Work off `main`. Branch names: `<type>/<short-description>`, where
  `<type>` is one of `feat`, `fix`, `docs`, `chore`, `refactor`, `test`.
  Examples: `feat/azure-backend`, `fix/gcs-scope-refresh`,
  `docs/readme-screenshots`.
- Don't push to `main` directly. Every change goes through a pull
  request, even from maintainers.

### Commit messages — Conventional Commits

We follow [Conventional Commits](https://www.conventionalcommits.org/)
so the changelog and version bumps can be generated automatically.

Format:

```
<type>(<optional scope>): <short summary>

<optional body — wrap at 72 cols, explain *why*, not *what*>

<optional footers, including BREAKING CHANGE: ...>
```

Common types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`,
`ci`, `build`, `perf`, `style`. Examples from this repo:

```
feat: add per-provider read-only mode
refactor: change folder creation task label
docs: update README.md
```

A `BREAKING CHANGE:` footer (or a `!` after the type, e.g.
`feat!: rename config field`) triggers a major version bump once we
reach `1.0`. Until then we use the same syntax but minor bumps may
still break compatibility — see the SemVer policy in the README.

### One concern per PR

Keep PRs focused. A formatting sweep, a refactor, and a behaviour change
should be three PRs, not one. Reviewers can move through small focused
PRs much faster.

---

## Submitting a pull request

1. Fork the repo (external contributors) or push your branch (maintainers).
2. Open a PR against `main`. Fill in the PR template — every checkbox is
   there for a reason.
3. Make sure CI is green: build + unit tests on Linux/macOS,
   `cargo fmt --check`, `cargo clippy -- -D warnings`.
4. Address review feedback by pushing additional commits — please do
   **not** force-push during review unless asked, it makes incremental
   review hard. The PR is squash-merged at the end, so the intermediate
   history doesn't pollute `main`.
5. If your change touches a destructive code path (`upload`,
   `create_folder`, `delete_objects`, anything that mutates remote
   state), double-check that **read-only mode still hard-blocks** it.
   Add or extend a test in `tests/test_read_only_storage.rs`.

### What gets merged faster

- Tests that cover the new behaviour (positive *and* negative cases).
- A README / docs update if user-visible behaviour changed.
- Optionally, a `CHANGELOG.md` entry under `## [Unreleased]`; the
  Conventional Commit subject is enough otherwise.
- A clear PR description: what problem, what approach, what was
  considered and rejected.

---

## Developer Certificate of Origin (DCO)

We use the [Developer Certificate of Origin](https://developercertificate.org/)
rather than a CLA. By adding a `Signed-off-by` line to your commits you
certify that you wrote the code (or have the right to submit it under
the project's license).

Sign off automatically with `git commit -s` (or `--signoff`). The line
looks like:

```
Signed-off-by: Jane Doe <jane@example.com>
```

The name and email must match your `git config user.name` /
`user.email`. We may add a DCO check to CI later; for now we'll ask you
to amend if a sign-off is missing.

---

## Coding guidelines

- **`cargo fmt` is the source of truth** for layout. Don't hand-format.
- **No clippy warnings.** If a lint is genuinely wrong for a specific
  call site, `#[allow(...)]` it with a one-line comment explaining why.
- **Errors:** prefer `anyhow::Result` at the application boundary,
  domain-specific error enums where it helps callers branch.
- **Logging:** use `tracing` (not `println!`). Keep `info!` for things
  the user might want to see, `debug!`/`trace!` for everything else.
- **Tests:** prefer integration-style tests against the public API
  surface. Unit-test the algorithms (path canonicalisation, RO-mode
  decoration, GCS scope construction); integration-test the full
  read/write flows against MinIO.
- **No new dependencies casually.** If you're adding one, mention it in
  the PR description with a brief rationale and a note on its
  maintenance status. License must be MIT, Apache-2.0, BSD-2/3, ISC, or
  Unicode-DFS.
- **Cross-platform.** Don't assume Linux paths or POSIX-only APIs.
  Crossterm and the `dirs` crate cover most of what we need.

---

## MSRV bump policy

The Minimum Supported Rust Version is currently **1.85**. It is enforced
by the `MSRV (1.85)` CI job (`cargo build` + the unit-test suite on the
pinned toolchain) and declared in three places that must move together:

1. `rust-version = "1.85"` in `Cargo.toml`.
2. `dtolnay/rust-toolchain@1.85.0` in `.github/workflows/ci.yml` (the
   `msrv` job).
3. The "Rust 1.85 or newer" line in `README.md` and the prerequisites
   section of this file.

### When you may bump MSRV

- **Required dependency upgrade** that no longer supports the current
  floor (Renovate will surface this on a routine version PR).
- **A standard-library API stabilised in a newer version** that meaningfully
  simplifies the code (e.g., `let-chains` in 1.88, `Option::is_none_or`
  in 1.82). Convenience alone is not enough — it has to remove
  meaningful complexity.
- **Security fix** in a Rust release that we want to recommend.

### Constraints on the bump

- Don't bump past `stable - 6 months` without a strong reason; many
  distros and CI environments lag.
- A bump is its own dedicated PR (`chore(msrv): bump to 1.X`) — never
  bundled into a feature commit.
- Update all three locations above in the same commit, plus this
  document, so they don't drift.

### Acceptable workarounds inside the codebase

If a clippy lint suggests a fix that would require a higher MSRV, prefer:

1. `#[allow(...)]` at the call site with a one-line `// MSRV: ...`
   comment, **or**
2. Add the lint to the `[lints.clippy]` allow-list in `Cargo.toml` with
   a comment explaining the MSRV reason (this is what we did for
   `collapsible_if`, whose suggested let-chain fix needs 1.88).

---

## Questions?

Open a [GitHub Discussion](https://github.com/thmswt/stuie/discussions)
or use the **Question** issue template if Discussions isn't enabled
yet.
