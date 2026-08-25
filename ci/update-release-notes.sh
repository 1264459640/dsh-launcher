#!/usr/bin/env bash
# Publishes the prepared release: renders the bilingual architecture download
# table with clickable asset links plus both changelog sections, then flips
# the release from draft to published (prerelease flag kept from prepare).
set -euo pipefail

RELEASE_ID="${1:?release id}"
TAG="${2:?tag}"
CHANGELOG_VERSION="${3:?changelog version}"
PRERELEASE="${4:?prerelease}"
REPO="${GITHUB_REPOSITORY:?}"
GH_TOKEN="${GH_TOKEN:?}"

# All uploaded assets; fail early if any name does not match the classifier.
# Fetch by numeric release id (REST) — `gh release view` resolves tag -> node_id
# for draft releases, and tag lookup 404s on untagged drafts.
ASSETS_JSON="$(gh api "repos/$REPO/releases/$RELEASE_ID" --jq '[.assets[].name]' | jq -c .)"

EN_CHANGELOG="$(awk -v ver="$CHANGELOG_VERSION" '$0 ~ "^## \\[" ver "\\]" {f=1} f {print} f && $0 ~ "^## \\[" && $0 !~ "^## \\[" ver "\\]" {exit}' CHANGELOG.md)"
ZH_CHANGELOG="$(awk -v ver="$CHANGELOG_VERSION" '$0 ~ "^## \\[" ver "\\]" {f=1} f {print} f && $0 ~ "^## \\[" && $0 !~ "^## \\[" ver "\\]" {exit}' CHANGELOG.zh_CN.md)"

node ci/release-notes-render.mjs "$TAG" "$ASSETS_JSON" "$EN_CHANGELOG" "$ZH_CHANGELOG" > notes.md

# Release was created published by resolve-release.sh; just refresh the notes.
# (Keeps the prerelease flag that prepare resolved.)
gh release edit "$RELEASE_ID" --repo "$REPO" --prerelease="$PRERELEASE" --notes-file notes.md