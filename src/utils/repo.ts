/**
 * Short display form of a SKILL repo URL: drops the protocol and host for
 * GitHub repos (`https://[user:password@]github.com/user/repo.git` →
 * `user/repo`), and trims the `.git` suffix / trailing slashes elsewhere.
 * The full URL stays available via tooltips; this is display-only.
 */
export function shortRepoName(url: string): string {
  const base = url.split('#')[0].trim()
  const stripped = base
    .replace(/^https?:\/\/(?:[^@/]+@)?github\.com\//i, '')
    .replace(/\.git$/i, '')
    .replace(/\/+$/, '')
  return stripped || base
}
