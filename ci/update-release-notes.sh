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
ASSETS_JSON="$(gh release view "$TAG" --repo "$REPO" --json assets -q '[.assets[].name]' | jq -c .)"

EN_CHANGELOG="$(awk -v ver="$CHANGELOG_VERSION" '$0 ~ "^## \\[" ver "\\]" {f=1} f {print} f && $0 ~ "^## \\[" && $0 !~ "^## \\[" ver "\\]" {exit}' CHANGELOG.md)"
ZH_CHANGELOG="$(awk -v ver="$CHANGELOG_VERSION" '$0 ~ "^## \\[" ver "\\]" {f=1} f {print} f && $0 ~ "^## \\[" && $0 !~ "^## \\[" ver "\\]" {exit}' CHANGELOG.zh_CN.md)"

node ci/release-notes-render.mjs "$TAG" "$ASSETS_JSON" "$EN_CHANGELOG" "$ZH_CHANGELOG" > notes.md

gh release edit "$RELEASE_ID" --repo "$REPO" --draft=false --prerelease="$PRERELEASE" --notes-file notes.md