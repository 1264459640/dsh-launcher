# DSH 整合包 Manifest 规范

一个整合包（modpack）是一个 `.tgz` 文件，描述并携带一个完整的 DSH Profile。压缩包**根目录**包含：

| 文件 | 必需 | 说明 |
| --- | --- | --- |
| `manifest.json` | ✅ | 整合包元数据与依赖坐标（本文档的主题） |
| `package.json` | ✅ | 可直接 `pnpm install` 的 profile 清单（由 `dependencies` 转换而来） |
| `cordis.patch.yml` | 否 | profile 的 patch 层；缺失时回退到 manifest 的 `patch` 字段 |
| `pnpm-lock.yaml` | 否 | 锁定传递依赖版本（导入时优先 `--frozen-lockfile`） |
| `pnpm-workspace.yaml` | 否 | profile 的 pnpm 设置（hoist、allowBuilds 等） |

当前规范版本为 **manifestVersion 3**；启动器同时兼容导入 manifestVersion 2。

## manifestVersion 3

### 字段定义

| 字段 | 类型 | 必需 | 说明 |
| --- | --- | --- | --- |
| `manifestVersion` | number | ✅ | 固定为 `3` |
| `name` | string | ✅ | 整合包标识（kebab-case），也是 tgz 文件名前缀 |
| `version` | string | ✅ | 整合包自身版本号（semver） |
| `displayName` | string \| object | 否 | 显示名。字符串，或以语言代码为键的 map（如 `{"zh-CN": "...", "en-US": "..."}`)；导入时按当前界面语言作为默认实例名 |
| `description` | string \| object | 否 | 描述，形式同 `displayName` |
| `author` | string | 否 | 作者 |
| `icon` | string | 否 | 图标（URL 或相对路径，预留） |
| `dshVersion` | string | 否 | **固定的精确版本号**（如 `0.1.1-rc.2`)，即导出该整合包时所用的 DSH 版本；导入时安装该版本。缺省时使用本机最新已装版本 |
| `profileName` | string | 否 | 导入时创建的 profile 名；缺省为 `pack`（保持 `web` profile 干净） |
| `bundles` | string[] | ✅ | profile 的 `dsh.profile.bundles` 层栈（按序挂载） |
| `dependencies` | object | ✅ | **坐标 → 固定版本**：npm 包为 `"包名": "精确版本"`;git 包为 `"github:owner/repo": "commit sha"`，monorepo 子目录用 `"github:owner/repo#path:/子目录"` |
| `patch` | string | 否 | `cordis.patch.yml` 的内联内容（与同名文件二选一，文件优先） |

### 示例

```json
{
  "manifestVersion": 3,
  "name": "all-about-whales",
  "displayName": {
    "en-US": "All About Whales",
    "zh-CN": "大肥鱼套装"
  },
  "version": "1.0.0",
  "description": {
    "en-US": "Make your DSH smell like big fat whales (beautify webUI with DeepSeek mascot theme)",
    "zh-CN": "让你的DSH充满大肥鱼的味道（用DeepSeek吉祥物主题美化webUI）"
  },
  "author": "hxh230802",
  "icon": "",
  "dshVersion": "0.1.1-rc.2",
  "profileName": "all-about-whales",
  "bundles": [
    "@deepseek-ai/dsh-base",
    "@deepseek-ai/dsh-web-app",
    "dafy-whale-theme",
    "dsh-whale-widget",
    "dsh-reasoning-effort",
    "dsh-pet"
  ],
  "dependencies": {
    "github:DViridescent/dafy-whale-theme": "99e8c57",
    "dsh-pet": "0.2.0",
    "github:HanaAyane/dsh-reasoning-effort": "83bc8c5",
    "github:MeteorNOX/DeepSeek-Balance-Whale-Widget": "4448c61"
  },
  "patch": "# Your patch layer for this dsh profile, applied after every bundle layer:\n# a top-level YAML array of loader patch entries (id-targeted config\n# overrides, disables, and insert lists; `!!js` expressions allowed).\n[]\n"
}
```

v3 的 `dependencies` 会在导入时转换为 pnpm 可安装的 package.json 形式：

- `"dsh-pet": "0.2.0"` → `"dsh-pet": "0.2.0"`（精确版本）
- `"github:owner/repo": "<sha>"` → `"repo": "github:owner/repo#<sha>"`
- `"github:owner/repo#path:/pkg": "<sha>"` → `"pkg": "github:owner/repo#<sha>&path:pkg"`

## manifestVersion 2（兼容）

v2 是 [ModPack-CLI](https://github.com/DSH-PackForge/ModPack-CLI) 写出的原始格式，与 v3 的差异：

| 字段 | v2 行为 |
| --- | --- |
| `displayName` / `description` | 仅字符串 |
| `dshVersion` | semver 范围（如 `>=0.1.0`)；导入时取其下限版本 |
| `dependencies` | 值为 pnpm 原始 spec(`^0.2.0`、`git+https://github.com/owner/repo.git` 等），键为包名；导入时原样透传 |

### 示例

```json
{
  "manifestVersion": 2,
  "name": "all-about-whales",
  "displayName": "大肥鱼套装",
  "version": "1.0.0",
  "description": "让你的DSH充满大肥鱼的味道（用DeepSeek吉祥物主题美化webUI）",
  "author": "hxh230802",
  "icon": "",
  "dshVersion": ">=0.1.0",
  "profileName": "all-about-whales",
  "bundles": [
    "@deepseek-ai/dsh-base",
    "@deepseek-ai/dsh-web-app",
    "dafy-whale-theme",
    "dsh-whale-widget",
    "dsh-reasoning-effort",
    "dsh-pet"
  ],
  "dependencies": {
    "dafy-whale-theme": "git+https://github.com/DViridescent/dafy-whale-theme.git",
    "dsh-pet": "^0.2.0",
    "dsh-reasoning-effort": "git+https://github.com/HanaAyane/dsh-reasoning-effort.git",
    "dsh-whale-widget": "git+https://github.com/MeteorNOX/DeepSeek-Balance-Whale-Widget.git"
  },
  "patch": "# ...\n[]\n"
}
```

## 导入行为

1. 读取 `manifest.json` 后弹出确认框：实例名（默认当前语言的 `displayName`)、profile 名（默认 `profileName`，缺省 `pack`)。
2. 为整合包新建实例与专属 DSH_HOME;`web` profile 保持纯净，整合包内容只进入 pack profile。
3. 安装 `dshVersion` 指定的 DSH 版本（未安装则自动下载；GitHub-only 标签走源码构建）。
4. 写入 `package.json` / `cordis.patch.yml` / `pnpm-lock.yaml` / `pnpm-workspace.yaml` 后执行 `pnpm install`（优先 `--frozen-lockfile`，失配时回退普通安装）。
5. pack profile 设为该实例的默认 profile。
