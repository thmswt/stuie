<!--
Thanks for the pull request! A few quick reminders:

1. Title follows Conventional Commits, e.g.:
     feat(gcs): support multi-project listing
     fix: stop crashing on empty bucket
     docs: clarify read-only mode behaviour
   Use `!` after the type (or a `BREAKING CHANGE:` footer) for breaking
   changes.

2. Sign your commits off (DCO) — `git commit -s`. Without a
   `Signed-off-by` line we'll ask you to amend.

3. If your change touches a destructive operation (upload, create
   folder, delete), make sure read-only mode still hard-blocks it and
   add or extend a test.
-->

## Summary

<!-- 1-3 sentences: what does this PR change, and why? Focus on the *why*
the diff already shows the *what*. -->

## Related issues

<!-- Closes #123 / Refs #456 / N/A -->

## Type of change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that changes existing behaviour or config schema)
- [ ] Refactor / internal cleanup (no user-visible change)
- [ ] Documentation only
- [ ] CI / build / tooling

## Checklist

- [ ] My commits follow [Conventional Commits](https://www.conventionalcommits.org/) and are signed off (`git commit -s`).
- [ ] I ran `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` locally with no findings.
- [ ] I added or updated tests covering the change (positive *and* negative paths where relevant).
- [ ] If this touches a destructive operation (`upload`, `create_folder`, `delete_objects`), I verified read-only mode still hard-blocks it and added/extended a test in `tests/test_read_only_storage.rs`.
- [ ] I updated user-facing documentation (`README.md`, in-app help, config examples) if behaviour changed.
- [ ] I considered cross-platform impact (Linux / macOS / Windows) where applicable.
- [ ] I have **not** introduced new dependencies without calling them out below.

## New dependencies

<!-- List any added crates with a one-line rationale and a note on the
license + maintenance status. Delete this section if none. -->

## Testing notes

<!-- How did you verify this works? Which test commands did you run?
For S3 changes, did you run the MinIO suite locally? For GCS changes
that need a real project, what did you test against? -->

## Screenshots / recordings

<!-- For UI-visible changes: a screenshot or short recording.
Delete this section if not applicable. -->

## Additional context

<!-- Anything reviewers should know that isn't obvious from the diff:
trade-offs, follow-ups, things explicitly out of scope. -->
