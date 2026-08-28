#!/usr/bin/env bash
# Publishes the prepared release: renders the English downloads table plus the
# "What's Changed" commit list, then refreshes the release notes (prerelease
# flag kept from prepare).
#
# Commit collection scope:
#   prerelease (v<ver>-dev.<n>)  -> commits since the previous tag of EITHER
#                                   kind — the nearest dev or release tag by
#                                   semantic version (a release outranks its
#                                   own dev tags). Right after a release the
#                                   first dev of the new cycle thus collects
#                                   everything since that release, while
#                                   later devs stay incremental.
#   release    (v<ver>)          -> commits since the previous release tag
#                                   (strict vX.Y.Z only; the newest dev tag
#                                   usually sits on the same commit and
#                                   would render "What's Changed" blank)
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

# The current TAG is always excluded so the previous one is picked even for
# re-runs. All filters use strict patterns — this repo carries historical
# test tags like v0.1.0-test3 / v0.1.0-step1 that must never qualify.
if [[ "$TAG" == *-dev.* ]]; then
  # Dev builds: base on the previous tag of EITHER kind — strict release or
  # dev prerelease — nearest by semantic version. git's version:refname sort
  # cannot be used here: it orders a dev tag ABOVE its own release
  # (v0.2.0-dev.49 > v0.2.0), which would anchor the first dev of a new
  # cycle to the PREVIOUS cycle's last dev tag and leak already-released
  # commits into the notes. Instead each candidate gets a sort key in which
  # the release sorts after its devs (suffix "zzzz" > "dev"), and GNU
  # sort -rV orders the keys semantically.
  PREV_TAG="$(
    git tag --list 'v*' \
      | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+(-dev\.[0-9]+)?$' \
      | grep -v -x "$TAG" \
      | sed -E 's/^v([0-9]+\.[0-9]+\.[0-9]+)$/\1-zzzz &/; t; s/^v([0-9]+\.[0-9]+\.[0-9]+-dev\.[0-9]+)$/\1 &/' \
      | sort -rV \
      | head -n 1 \
      | cut -d' ' -f2- || true
  )"
else
  # Releases: strict vX.Y.Z only. A dev tag always sorts below its release
  # version and the release tag is typically cut on the same commit as the
  # last dev tag, so a same-commit dev base would produce an empty range and
  # a blank "What's Changed".
  PREV_TAG="$(
    git tag --list 'v*' --sort=-version:refname \
      | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
      | grep -v -x "$TAG" \
      | head -n 1 || true
  )"
fi

if [[ -n "$PREV_TAG" ]]; then
  echo "collecting commits since $PREV_TAG"
  # Subject-only commit list, newest first, skipping merge commits.
  git log --no-merges --format='%s' "$PREV_TAG..HEAD" > commits.txt
else
  echo "no previous tag found; listing all commits"
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
