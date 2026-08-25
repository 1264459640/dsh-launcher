// Release-note renderer + artifact classifier.
// Shared by ci/update-release-notes.sh (CLI) and ci/release-notes.test.mjs (tests).

/**
 * Maps a release asset file name to a { platform, arch, kind, name } record,
 * or null when the name does not match any known artifact.
 */
export function classify(name) {
  // RPM uses a different scheme: dsh-launcher-0.1.0-1.x86_64.rpm
  const rpm = /^dsh-launcher-\d[\w.-]*\.(x86_64|aarch64)\.rpm$/.exec(name)
  if (rpm) {
    return { name, arch: rpm[1] === 'x86_64' ? 'x86_64' : 'arm64', kind: 'rpm' }
  }
  const m = /^dsh-launcher_\d[\w.-]*_(x64|arm64|amd64|aarch64).*\.(exe|msi|AppImage|deb|rpm|dmg)$/.exec(name)
  if (!m) return null
  const archRaw = m[1]
  return {
    name,
    arch: archRaw === 'amd64' || archRaw === 'x64' ? 'x86_64' : 'arm64',
    kind: m[2],
  }
}

const PLATFORM_OF_KIND = { exe: 'Windows', msi: 'Windows', AppImage: 'Linux', deb: 'Linux', rpm: 'Linux', dmg: 'macOS' }
const KIND_LABEL_EN = { exe: 'NSIS setup', msi: 'MSI', AppImage: 'AppImage', deb: 'DEB', rpm: 'RPM', dmg: 'DMG' }
const KIND_LABEL_ZH = { exe: 'NSIS 安装包', msi: 'MSI 安装包', AppImage: 'AppImage', deb: 'DEB 包', rpm: 'RPM 包', dmg: 'DMG 镜像' }
const KIND_ORDER = { exe: 0, msi: 1, AppImage: 2, deb: 3, rpm: 4, dmg: 5 }

/**
 * Renders the bilingual "Downloads" table plus both changelog sections.
 * Every artifact row links directly to its release-asset download URL.
 */
export function render(tag, assets, changelogEn, changelogZh, { repo = 'REPO', version = '' } = {}) {
  const sorted = [...assets]
    .filter((a) => classify(a))
    .sort((a, b) => {
      const c1 = classify(a)
      const c2 = classify(b)
      if (c1.arch !== c2.arch) return c1.arch.localeCompare(c2.arch)
      return KIND_ORDER[c1.kind] - KIND_ORDER[c2.kind]
    })

  const link = (name) => `https://github.com/${repo}/releases/download/${tag}/${name}`
  const rowsEn = sorted.map((name) => {
    const c = classify(name)
    return `| ${PLATFORM_OF_KIND[c.kind]} | ${c.arch} | [${name}](${link(name)}) (${KIND_LABEL_EN[c.kind]}) |`
  })
  const rowsZh = sorted.map((name) => {
    const c = classify(name)
    return `| ${PLATFORM_OF_KIND[c.kind]} | ${c.arch} | [${name}](${link(name)})（${KIND_LABEL_ZH[c.kind]}）|`
  })

  return `## Downloads

### English

| Platform | Architecture | File |
| --- | --- | --- |
${rowsEn.join('\n')}

### 简体中文

| 平台 | 架构 | 文件 |
| --- | --- | --- |
${rowsZh.join('\n')}

---

${changelogEn.trim()}

---

${changelogZh.trim()}
`
}

// CLI mode: node ci/release-notes-render.mjs <tag> <assets-json> <changelog-en> <changelog-zh>
import { fileURLToPath, pathToFileURL } from 'node:url'
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [, , tag, assetsJson, en, zh] = process.argv
  const assets = JSON.parse(assetsJson)
  const repo = process.env.GITHUB_REPOSITORY ?? 'REPO'
  const unknown = assets.filter((a) => !classify(a))
  if (unknown.length > 0) {
    console.error(`release-note classifier rejected assets: ${unknown.join(', ')}`)
    process.exit(1)
  }
  process.stdout.write(render(tag, assets, en, zh, { repo }))
}