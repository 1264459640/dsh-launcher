#!/usr/bin/env bash
# Resolves an idempotent GitHub Release for this run.
#
#   branch push (main)  -> draft prerelease  v<manifest>-dev.<run_number>
#   exact version tag   -> draft stable      v<manifest> (tag must match manifest)
#
# The release stays a draft until update-release-notes.sh publishes it after
# every build job succeeds. Re-running the same run reuses the same release.
set -euo pipefail

REF_TYPE="${1:?ref type}"      # branch | tag
REF_NAME="${2:?ref name}"      # main | v0.1.0
RUN_NUMBER="${3:?run number}"
REPO="${GITHUB_REPOSITORY:?}"
GH_TOKEN="${GH_TOKEN:?}"

# Manifest synchronization + changelog section requirement.
node ci/check-versions.mjs
MANIFEST_VERSION="$(node -p "require('./package.json').version")"
grep -q "^## \[${MANIFEST_VERSION}\]" CHANGELOG.md \
  || { echo "error: CHANGELOG.md is missing section ## [${MANIFEST_VERSION}]" >&2; exit 1; }
grep -q "^## \[${MANIFEST_VERSION}\]" CHANGELOG.zh_CN.md \
  || { echo "error: CHANGELOG.zh_CN.md is missing section ## [${MANIFEST_VERSION}]" >&2; exit 1; }

if [[ "$REF_TYPE" == "tag" ]]; then
  TAG="$REF_NAME"
  VERSION="${TAG#v}"
  [[ "$VERSION" == "$MANIFEST_VERSION" ]] \
    || { echo "error: tag ${TAG} does not match manifest version ${MANIFEST_VERSION}" >&2; exit 1; }
  case "$VERSION" in *-*) PRERELEASE="true";; *) PRERELEASE="false";; esac
else
  VERSION="$MANIFEST_VERSION"
  TAG="v${VERSION}-dev.${RUN_NUMBER}"
  PRERELEASE="true"
fi

# Idempotent: reuse the release if the tag already exists (re-run of a run).
EXISTING_ID="$(gh release view "$TAG" --repo "$REPO" --json id -q .id 2>/dev/null || true)"
if [[ -n "$EXISTING_ID" ]]; then
  RELEASE_ID="$EXISTING_ID"
else
  # gh release create has no --json; create first (discard output), then fetch the id.
  gh release create "$TAG" --repo "$REPO" --draft --prerelease="$PRERELEASE" --title "$TAG" --notes "Preparing artifacts…" >/dev/null
  RELEASE_ID="$(gh release view "$TAG" --repo "$REPO" --json id -q .id)"
fi

echo "release_id=$RELEASE_ID" >> "$GITHUB_OUTPUT"
echo "tag=$TAG" >> "$GITHUB_OUTPUT"
echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "changelog_version=$MANIFEST_VERSION" >> "$GITHUB_OUTPUT"
echo "prerelease=$PRERELEASE" >> "$GITHUB_OUTPUT"