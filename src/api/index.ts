// API layer: talks to the Tauri backend via invoke when running inside the
// desktop shell, and falls back to a localStorage-backed mock in a plain
// browser so the UI can be previewed without the Rust side.

import type {
  DshHome,
  DshInstance,
  DshVersion,
  InstallProgress,
  InstanceStatus,
  LauncherSettings,
  NewInstanceInput,
  RemoteVersion,
} from './types'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// ---------------------------------------------------------------------------
// Mock backend (browser preview)
// ---------------------------------------------------------------------------

const MOCK_KEY = 'dsh-launcher.mock.v1'

interface MockDb {
  homes: DshHome[]
  versions: DshVersion[]
  instances: DshInstance[]
  settings: LauncherSettings
  running: Record<string, InstanceStatus>
}

function seedDb(): MockDb {
  return {
    homes: [
      { id: 'h-default', name: '默认 DSH_HOME', path: 'C:\\Users\\Administrator\\.dsh' },
      { id: 'h-lab', name: '实验室环境', path: 'D:\\dsh-homes\\lab' },
    ],
    versions: [
      { id: 'v-rc6', version: '0.1.0-rc.6', dir: 'C:\\Users\\Administrator\\AppData\\Roaming\\dsh-launcher\\versions\\0.1.0-rc.6' },
      { id: 'v-rc5', version: '0.1.0-rc.5', dir: 'C:\\Users\\Administrator\\AppData\\Roaming\\dsh-launcher\\versions\\0.1.0-rc.5' },
    ],
    instances: [
      {
        id: 'i-main',
        name: '主实例',
        version_id: 'v-rc6',
        home_id: 'h-default',
        env_overrides: { DSH_TELEMETRY_DISABLED: '1' },
        default_profile: 'web',
        last_profile: 'web',
      },
      {
        id: 'i-exp',
        name: '实验实例',
        version_id: 'v-rc5',
        home_id: 'h-lab',
        env_overrides: {},
        default_profile: null,
        last_profile: null,
      },
    ],
    settings: {
      locale: 'zh-CN',
      minimize_to_tray: true,
      autostart: false,
      last_instance_id: 'i-main',
    },
    running: {},
  }
}

function loadDb(): MockDb {
  try {
    const raw = localStorage.getItem(MOCK_KEY)
    if (raw) return JSON.parse(raw) as MockDb
  } catch {
    // fall through to seed
  }
  const db = seedDb()
  localStorage.setItem(MOCK_KEY, JSON.stringify(db))
  return db
}

function saveDb(db: MockDb) {
  localStorage.setItem(MOCK_KEY, JSON.stringify(db))
}

function uuid(): string {
  return 'xxxxxxxx-xxxx-4xxx'.replace(/x/g, () => ((Math.random() * 16) | 0).toString(16))
}

// Simple event emitter used by the mock to mimic Tauri events.
type Listener<T> = (payload: T) => void
const statusListeners = new Set<Listener<InstanceStatus>>()
const progressListeners = new Set<Listener<InstallProgress>>()

function emitStatus(s: InstanceStatus) {
  statusListeners.forEach((fn) => fn(s))
}
function emitProgress(p: InstallProgress) {
  progressListeners.forEach((fn) => fn(p))
}

// ---------------------------------------------------------------------------
// Tauri invoke wrapper
// ---------------------------------------------------------------------------

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<T>(cmd, args)
  }
  return mockCall<T>(cmd, args)
}

async function mockCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const db = loadDb()
  function fail(msg: string): never {
    throw new Error(msg)
  }

  switch (cmd) {
    case 'list_homes':
      return db.homes as T
    case 'default_dedicated_home_path': {
      const name = String(args?.name ?? 'instance')
      const safe = name.replace(/[^\w一-龥.-]+/g, '_')
      return `C:\\Users\\Administrator\\AppData\\Roaming\\dsh-launcher\\homes\\${safe}` as T
    }
    case 'create_home': {
      const name = String(args?.name ?? '').trim()
      const path = String(args?.path ?? '').trim()
      if (!name || !path) fail('名称与路径不能为空')
      const home: DshHome = { id: `h-${uuid()}`, name, path }
      db.homes.push(home)
      saveDb(db)
      return home as T
    }
    case 'remove_home': {
      const id = String(args?.id)
      if (db.instances.some((i) => i.home_id === id)) fail('该 DSH_HOME 仍被实例引用，无法删除')
      db.homes = db.homes.filter((h) => h.id !== id)
      saveDb(db)
      return undefined as T
    }
    case 'list_versions':
      return db.versions as T
    case 'fetch_available_versions':
      return [
        { version: '0.1.0-rc.6', released_at: '2026-08-01T12:00:00Z' },
        { version: '0.1.0-rc.5', released_at: '2026-07-15T09:30:00Z' },
        { version: '0.1.0-rc.4', released_at: '2026-07-01T10:00:00Z' },
        { version: '0.1.0-rc.3', released_at: '2026-06-15T08:00:00Z' },
      ] as T
    case 'install_version': {
      const version = String(args?.version)
      if (db.versions.some((v) => v.version === version)) fail(`版本 ${version} 已安装`)
      const v: DshVersion = {
        id: `v-${uuid()}`,
        version,
        dir: `C:\\Users\\Administrator\\AppData\\Roaming\\dsh-launcher\\versions\\${version}`,
      }
      // Simulate progress; the command resolves once the install completes.
      return new Promise((resolve) => {
        let pct = 0
        emitProgress({ version, percent: 0, stage: 'downloading', message: null })
        const timer = setInterval(() => {
          pct += 20
          const done = pct >= 100
          emitProgress({ version, percent: Math.min(pct, 100), stage: done ? 'done' : 'installing', message: null })
          if (done) {
            clearInterval(timer)
            db.versions.push(v)
            saveDb(db)
            resolve(v)
          }
        }, 300)
      }) as T
    }
    case 'remove_version': {
      const id = String(args?.id)
      if (db.instances.some((i) => i.version_id === id)) fail('该版本仍被实例引用，无法删除')
      db.versions = db.versions.filter((v) => v.id !== id)
      saveDb(db)
      return undefined as T
    }
    case 'list_instances':
      return db.instances as T
    case 'create_instance': {
      const input = args?.input as NewInstanceInput
      if (db.instances.some((i) => i.name === input.name)) fail('同名实例已存在')
      const inst: DshInstance = {
        id: `i-${uuid()}`,
        name: input.name,
        version_id: input.version_id,
        home_id: input.home_id,
        env_overrides: input.env_overrides ?? {},
        default_profile: input.default_profile ?? null,
        last_profile: null,
      }
      db.instances.push(inst)
      saveDb(db)
      return inst as T
    }
    case 'update_instance': {
      const input = args?.input as DshInstance
      if (db.instances.some((i) => i.name === input.name && i.id !== input.id)) fail('同名实例已存在')
      db.instances = db.instances.map((i) => (i.id === input.id ? input : i))
      saveDb(db)
      return input as T
    }
    case 'delete_instance': {
      const id = String(args?.id)
      delete db.running[id]
      db.instances = db.instances.filter((i) => i.id !== id)
      if (db.settings.last_instance_id === id) db.settings.last_instance_id = null
      saveDb(db)
      return undefined as T
    }
    case 'list_profiles': {
      const homeId = String(args?.home_id)
      const home = db.homes.find((h) => h.id === homeId)
      if (!home) fail('DSH_HOME 不存在')
      // Mock: the default home has the real profile set; others are empty.
      if (home.path.endsWith('.dsh')) return ['web', 'demo', 'pack'] as T
      return ['web'] as T
    }
    case 'start_instance': {
      const id = String(args?.id)
      const profile = String(args?.profile)
      if (db.running[id]?.state === 'running' || db.running[id]?.state === 'starting') fail('实例已在运行')
      const inst = db.instances.find((i) => i.id === id)
      if (!inst) fail('实例不存在')
      inst.last_profile = profile
      const starting: InstanceStatus = { id, state: 'starting', url: null, profile, exit_code: null }
      db.running[id] = starting
      saveDb(db)
      emitStatus(starting)
      setTimeout(() => {
        const cur = loadDb()
        if (cur.running[id]?.state !== 'starting') return
        const running: InstanceStatus = {
          id,
          state: 'running',
          url: `http://127.0.0.1:${30000 + Math.floor(Math.random() * 20000)}`,
          profile,
          exit_code: null,
        }
        cur.running[id] = running
        saveDb(cur)
        emitStatus(running)
      }, 1500)
      return undefined as T
    }
    case 'stop_instance': {
      const id = String(args?.id)
      const stopped: InstanceStatus = { id, state: 'stopped', url: null, profile: null, exit_code: 0 }
      delete db.running[id]
      saveDb(db)
      emitStatus(stopped)
      return undefined as T
    }
    case 'open_instance_window': {
      const id = String(args?.id)
      const status = db.running[id]
      if (status?.url) window.open(status.url, '_blank')
      return undefined as T
    }
    case 'list_instance_status':
      return Object.values(db.running) as T
    case 'get_settings':
      return db.settings as T
    case 'update_settings': {
      db.settings = { ...db.settings, ...(args?.settings as Partial<LauncherSettings>) }
      saveDb(db)
      return db.settings as T
    }
    default:
      fail(`mock: unknown command ${cmd}`)
  }
}
// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export const api = {
  isTauri,

  listHomes: () => call<DshHome[]>('list_homes'),
  createHome: (name: string, path: string) => call<DshHome>('create_home', { name, path }),
  removeHome: (id: string) => call<void>('remove_home', { id }),
  defaultDedicatedHomePath: (name: string) => call<string>('default_dedicated_home_path', { name }),

  listVersions: () => call<DshVersion[]>('list_versions'),
  fetchAvailableVersions: () => call<RemoteVersion[]>('fetch_available_versions'),
  installVersion: (version: string) => call<DshVersion>('install_version', { version }),
  removeVersion: (id: string) => call<void>('remove_version', { id }),

  listInstances: () => call<DshInstance[]>('list_instances'),
  createInstance: (input: NewInstanceInput) => call<DshInstance>('create_instance', { input }),
  updateInstance: (input: DshInstance) => call<DshInstance>('update_instance', { input }),
  deleteInstance: (id: string) => call<void>('delete_instance', { id }),

  listProfiles: (homeId: string) => call<string[]>('list_profiles', { home_id: homeId }),

  startInstance: (id: string, profile: string) => call<void>('start_instance', { id, profile }),
  stopInstance: (id: string) => call<void>('stop_instance', { id }),
  openInstanceWindow: (id: string) => call<void>('open_instance_window', { id }),
  listInstanceStatus: () => call<InstanceStatus[]>('list_instance_status'),

  getSettings: () => call<LauncherSettings>('get_settings'),
  updateSettings: (settings: Partial<LauncherSettings>) => call<LauncherSettings>('update_settings', { settings }),

  async onInstanceStatus(cb: Listener<InstanceStatus>): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event')
      const un = await listen<InstanceStatus>('instance://status', (e) => cb(e.payload))
      return un
    }
    statusListeners.add(cb)
    return () => statusListeners.delete(cb)
  },

  async onInstallProgress(cb: Listener<InstallProgress>): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event')
      const un = await listen<InstallProgress>('version://install-progress', (e) => cb(e.payload))
      return un
    }
    progressListeners.add(cb)
    return () => progressListeners.delete(cb)
  },
}
