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
3. Make sure CI is green: build + unit tests on Linux/macOS/Windows,
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
- A `CHANGELOG.md` entry under `## [Unreleased]` once the release
  tooling lands; until then the Conventional Commit subject is enough.
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
  Unicode-DFS (see `deny.toml` once it lands).
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
  bundled into a feature commit. The release notes for the affected
  release must call it out.
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

## Releasing (maintainers only)

Release tooling: **`cargo-release`** for the version bump + tag +
changelog rollover + crates.io publish, and **`cargo-dist`** (now
distributed as `dist`) for cross-platform binary builds attached to a
GitHub Release. Configuration lives in `release.toml`,
`dist-workspace.toml`, and `CHANGELOG.md` (the sentinel comments).

> **Note:** dist reads its config from `dist-workspace.toml` only. A
> `[package.metadata.dist]` table in `Cargo.toml` is silently ignored
> (dist accepts those keys in *workspace* metadata), so don't add one —
> it looks authoritative and does nothing.

### One-time setup per maintainer

```bash
cargo install cargo-release
# `dist` (cargo-dist) — pin to the version declared in
# dist-workspace.toml's [dist].cargo-dist-version so the workflow you
# generate locally matches the one CI runs.
cargo install cargo-dist --version 0.32.0 --locked
cargo login                # crates.io token; needs publish rights
gh auth login              # to push tags and create releases via gh, if used
```

### One-time repo setup (already done — for reference)

`dist init` writes the config and generates
`.github/workflows/release.yml`. Re-run it whenever
`cargo-dist-version`, `targets`, or `installers` in
`dist-workspace.toml` changes:

```bash
dist init --yes  # non-interactive; keeps the values in dist-workspace.toml
dist generate    # regenerates the workflow from current config
dist plan        # dry-run: prints the artifacts a tag would produce
git add .github/workflows/release.yml dist-workspace.toml
git commit -m "chore(release): regenerate dist workflow for v0.X.0"
```

`dist init` will rewrite `dist-workspace.toml` with its own defaults if
a key is missing, so re-read the file after running it and restore
anything it dropped (`pr-run-mode`, `checksum`, the installer list).

### Cutting a release

1. Make sure `main` is green and your local checkout is up to date.
2. Move all relevant lines from `## [Unreleased]` into the categories
   they belong under (Added / Changed / Deprecated / Removed / Fixed /
   Security). Anything that doesn't belong in user-facing notes
   (typo fixes, internal refactors) can be omitted.
3. Dry-run first — cargo-release prints what it *would* do without
   touching anything:

   ```bash
   cargo release patch          # or `minor` / `major`
   ```

4. Execute when the dry-run looks right:

   ```bash
   cargo release patch --execute
   ```

   This bumps `Cargo.toml`, applies the CHANGELOG sentinel
   replacements, creates the `chore(release): vX.Y.Z` commit, tags
   `vX.Y.Z`, pushes both, and runs `cargo publish`.

5. The pushed tag triggers `dist`'s release workflow, which builds
   binaries for the configured targets, generates `install.sh` /
   `install.ps1`, uploads everything to a GitHub Release with
   checksums, and marks the release as latest.

6. Verify on the GitHub Releases page and on
   `https://crates.io/crates/stuie` that the new version is live.

### Choosing the bump level (0.x policy)

- **patch** (`0.X.y → 0.X.(y+1)`): bug fixes only, no behaviour or
  config-schema changes.
- **minor** (`0.X.y → 0.(X+1).0`): anything else, including breaking
  changes. While we're on `0.x`, minor bumps may break compat —
  that's documented in the README's *Versioning & Stability* section.
- **major** (`0.X.y → 1.0.0`): only when the criteria for `1.0` listed
  in the README are met.

### When something goes wrong

- **Dry-run shows a CHANGELOG replacement that didn't fire** (`min`/
  `exactly` mismatch): the sentinel comments in `CHANGELOG.md` were
  edited or removed. Restore the `<!-- next-header -->` and
  `<!-- next-url -->` markers and re-run the dry-run.
- **`cargo publish` fails on metadata**: usually a missing field after
  someone trimmed Cargo.toml. The required set is in section 6 of
  `PLAN.md`.
- **Tag exists but workflow never ran**: the tag wasn't pushed (cargo-
  release uses your git remote; check `git push --tags` succeeded), or
  `dist`'s workflow was disabled. Push manually with
  `git push origin vX.Y.Z` and the workflow will fire.
- **Need to roll back a release on crates.io**: you cannot delete a
  published version, but you can yank it (`cargo yank --version X.Y.Z`)
  to stop new resolutions from picking it. Yanked versions remain
  buildable for users who already depend on them.

---

## Questions?

Open a [GitHub Discussion](https://github.com/thmswt/stuie/discussions)
or use the **Question** issue template if Discussions isn't enabled
yet.
