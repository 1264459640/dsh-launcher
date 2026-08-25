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

# Find an existing release for this tag (draft or not) so a re-run of the
# same run number reuses it instead of creating a duplicate.
EXISTING_ID="$(gh release view "$TAG" --repo "$REPO" --json id -q .id 2>/dev/null || true)"
if [[ -n "$EXISTING_ID" ]]; then
  RELEASE_ID="$EXISTING_ID"
  echo "reusing existing release $RELEASE_ID ($TAG)"
else
  gh release create "$TAG" --repo "$REPO" --draft --prerelease="$PRERELEASE" --title "$TAG" --notes "Preparing artifacts…" >/dev/null
  RELEASE_ID="$(gh release view "$TAG" --repo "$REPO" --json id -q .id)"
  echo "created release $RELEASE_ID ($TAG)"
fi

# In CI, build jobs run tauri-action in parallel right after this job. It
# locates a draft release by listing all releases and matching tag_name, so
# an eventual-consistency delay here would make every build fail with
# "Release not found or created". Poll until the release is visible in the
# release list (bounded).
for i in $(seq 1 20); do
  if gh release list --repo "$REPO" --json tagName -q '.[].tagName' 2>/dev/null | grep -qx "$TAG"; then
    echo "release $TAG visible in release list after ${i} poll(s)"
    break
  fi
  if [[ "$i" == 20 ]]; then
    echo "error: release $TAG never became visible in the release list" >&2
    exit 1
  fi
  sleep 3
done

echo "release_id=$RELEASE_ID" >> "$GITHUB_OUTPUT"
echo "tag=$TAG" >> "$GITHUB_OUTPUT"
echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "changelog_version=$MANIFEST_VERSION" >> "$GITHUB_OUTPUT"
echo "prerelease=$PRERELEASE" >> "$GITHUB_OUTPUT"