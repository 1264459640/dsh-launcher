#!/usr/bin/env bash
# Publishes the prepared release: renders the English downloads table plus the
# "What's Changed" commit list, then refreshes the release notes (prerelease
# flag kept from prepare).
#
# Commit collection scope:
#   prerelease (v<ver>-dev.<n>)  -> commits since the previous prerelease tag
#   release    (v<ver>)          -> commits since the previous release tag
set -euo pipefail

RELEASE_ID="${1:?release id}"
TAG="${2:?tag}"
PRERELEASE="${3:?prerelease}"
REPO="${GITHUB_REPOSITORY:?}"
GH_TOKEN="${GH_TOKEN:?}"

# All uploaded assets; fail early if any name does not match the classifier.
# Fetch by numeric release id (REST) — `gh release view` resolves tag -> node_id
# for draft releases, and tag lookup 404s on untagged drafts.
ASSETS_JSON="$(gh api "repos/$REPO/releases/$RELEASE_ID" --jq '[.assets[].name]' | jq -c .)"

# Make sure the full history (not just the shallow HEAD) and all tags are
# present so `git log <prev>..HEAD` can traverse back to the previous tag.
if git rev-parse --is-shallow-repository | grep -q true; then
  git fetch --unshallow --tags --force --quiet
else
  git fetch --tags --force --quiet
fi

# Same-kind tags: prerelease tags are v<ver>-dev.<n>, release tags are plain
# v<ver> (no "-dev." suffix).
if [[ "$TAG" == *-dev.* ]]; then
  KIND_FILTER='v*-dev.*'
else
  KIND_FILTER='v*' # releases: plain v<ver> tags
fi

# Previous tag of the same kind, ordered by git tag version sort; the current
# TAG is excluded so the previous one is picked even for re-runs.
PREV_TAG="$(
  git tag --list "$KIND_FILTER" --sort=-version:refname \
    | grep -v -x "$TAG" \
    | head -n 1 || true
)"

if [[ -n "$PREV_TAG" ]]; then
  echo "collecting commits since $PREV_TAG"
  # Subject-only commit list, newest first, skipping merge commits.
  git log --no-merges --format='%s' "$PREV_TAG..HEAD" > commits.txt
else
  echo "no previous $([[ "$TAG" == *-dev.* ]] && echo prerelease || echo release) tag found; listing all commits"
  git log --no-merges --format='%s' > commits.txt
fi

node ci/release-notes-render.mjs "$TAG" "$ASSETS_JSON" commits.txt > notes.md

# Release was created published by resolve-release.sh; just refresh the notes.
# (Keeps the prerelease flag that prepare resolved.)
# REST PATCH by numeric id (like the GugleFS pipeline): `gh release edit`
# resolves its argument as a TAG, and a bare numeric id is not a tag, so it
# fails with "release not found" even though the release exists.
gh api -X PATCH "repos/$REPO/releases/$RELEASE_ID" \
  -F body=@notes.md \
  -F prerelease="$PRERELEASE" \
  --silent
