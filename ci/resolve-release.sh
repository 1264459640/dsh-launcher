#!/usr/bin/env bash
# Resolves an idempotent GitHub Release for this run.
#
#   branch push (main)  -> prerelease  v<manifest>-dev.<run_number>
#   exact version tag   -> stable      v<manifest> (tag must match manifest)
#
# The release is created PUBLISHED (draft=false) with a real git tag, exactly
# like the reference GugleFS pipeline. Draft releases are deliberately
# avoided: their tag association is eventual-consistency and tauri-action's
# parallel builds race it ("Release not found or created"). tauri-action then
# uploads straight to the numeric release id, no tag lookup needed.
set -euo pipefail

REF_TYPE="${1:?ref type}"      # branch | tag
REF_NAME="${2:?ref name}"      # main | v0.1.0
RUN_NUMBER="${3:?run number}"
REPO="${GITHUB_REPOSITORY:?}"
GH_TOKEN="${GH_TOKEN:?}"

# Manifest synchronization.
node ci/check-versions.mjs
MANIFEST_VERSION="$(node -p "require('./package.json').version")"

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

# Idempotent: reuse the release if the tag already has one (re-run of a run).
RELEASE_ID="$(gh api "repos/$REPO/releases?per_page=100" --jq ".[] | select(.tag_name == \"$TAG\") | .id" 2>/dev/null | head -n 1 || true)"
if [[ -n "$RELEASE_ID" ]]; then
  echo "reusing existing release $RELEASE_ID ($TAG)"
else
  # Create a published release; GitHub creates the git tag from target_commitish.
  RELEASE_ID="$(gh api "repos/$REPO/releases" \
    -f tag_name="$TAG" \
    -f target_commitish="$GITHUB_SHA" \
    -f name="$TAG" \
    -f body="Preparing artifacts…" \
    -F draft=false \
    -F prerelease="$PRERELEASE" \
    --jq .id)"
  echo "created release $RELEASE_ID ($TAG, prerelease=$PRERELEASE)"
fi

if [[ -z "$RELEASE_ID" ]]; then
  echo "error: could not resolve numeric id for release $TAG" >&2
  exit 1
fi

echo "release_id=$RELEASE_ID" >> "$GITHUB_OUTPUT"
echo "tag=$TAG" >> "$GITHUB_OUTPUT"
echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "prerelease=$PRERELEASE" >> "$GITHUB_OUTPUT"