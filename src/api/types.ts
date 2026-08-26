// Shared types mirrored from the Rust backend (src-tauri/src/config.rs).

export interface DshHome {
  id: string
  name: string
  path: string
}

export interface DshVersion {
  id: string
  version: string
  dir: string
}

export interface DshInstance {
  id: string
  name: string
  version_id: string
  home_id: string
  env_overrides: Record<string, string>
  default_profile: string | null
  last_profile: string | null
}

export interface LauncherSettings {
  locale: string
  minimize_to_tray: boolean
  autostart: boolean
  last_instance_id: string | null
  news_source: string
  theme: ThemeMode
}

/** UI theme: explicit light/dark, or follow the OS color scheme. */
export type ThemeMode = 'light' | 'dark' | 'system'

export type InstanceState = 'stopped' | 'starting' | 'running' | 'exited'

export interface InstanceStatus {
  id: string
  state: InstanceState
  url: string | null
  profile: string | null
  exit_code: number | null
}

export interface ToolStatus {
  installed: boolean
  version: string | null
  path: string | null
}

export interface RuntimeStatus {
  node: ToolStatus
  pnpm: ToolStatus
}

export type TaskState = 'running' | 'done' | 'error' | 'cancelled'

export interface TaskInfo {
  id: string
  kind: string
  label: string
  version: string
  state: TaskState
  percent: number
  created_at: number
  message: string | null
  instance_id: string | null
  instance_name: string | null
  logs: string[]
}

export interface TaskProgress {
  id: string
  state: TaskState
  percent: number
  message: string | null
  instance_id: string | null
}

export interface TaskLog {
  id: string
  line: string
}

export interface RemoteVersion {
  version: string
  released_at: string | null
}

export interface NewInstanceInput {
  name: string
  version_id: string
  home_id: string
  env_overrides: Record<string, string>
  default_profile: string | null
}

/** Input for duplicating an instance (new name + reuse/new DSH_HOME choice). */
export interface CopyInstanceInput {
  source_id: string
  name: string
  new_home: boolean
}

// ---------------------------------------------------------------------------
// Plugin marketplace
// ---------------------------------------------------------------------------

export interface MarketPluginDescription {
  language: string
  content: string
}

export interface MarketPluginUrls {
  homepage?: string
  repository?: string
  issues?: string
}

export interface MarketPluginRelationship {
  kind: string // "dependency" | "incompatibility"
  id: string
  versions: string
}

export interface MarketPlugin {
  id: string
  name: string
  description?: string | MarketPluginDescription[]
  support_versions?: unknown
  urls?: MarketPluginUrls
  relationship?: MarketPluginRelationship[]
}

export type PluginChannel = 'stable' | 'beta' | 'alpha'

export interface PluginVersionInfo {
  version: string
  channel: PluginChannel
  label?: string
  is_default: boolean
}

/** A page of versions; `has_more` enables infinite scrolling (alpha channel). */
export interface PluginVersionPage {
  versions: PluginVersionInfo[]
  has_more: boolean
}

export interface InstalledPlugin {
  id: string
  version?: string
  enabled: boolean
  cordis_id?: string
}

export interface InstallPluginInput {
  pluginId: string
  version: string
  channel: PluginChannel
  instanceId: string
  profile: string
}

export interface SetPluginsEnabledInput {
  instanceId: string
  profile: string
  pluginIds: string[]
  enabled: boolean
}
