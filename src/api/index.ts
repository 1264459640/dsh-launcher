// API layer: talks to the Tauri backend via invoke when running inside the
// desktop shell, and falls back to a localStorage-backed mock in a plain
// browser so the UI can be previewed without the Rust side.

import type {
  DshHome,
  DshInstance,
  DshVersion,
  InstanceStatus,
  LauncherSettings,
  NewInstanceInput,
  RemoteVersion,
  RuntimeStatus,
  TaskInfo,
  TaskLog,
  TaskProgress,
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
      { id: 'v-rc6', version: '0.1.0-rc.6', dir: 'C:\\Users\\Administrator\\AppData\\Roaming\\in.dsh-plug.dsh-launcher\\versions\\0.1.0-rc.6' },
      { id: 'v-rc5', version: '0.1.0-rc.5', dir: 'C:\\Users\\Administrator\\AppData\\Roaming\\in.dsh-plug.dsh-launcher\\versions\\0.1.0-rc.5' },
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
      news_source: 'https://gist.githubusercontent.com/Gu-ZT/f08daa33afb82f4b375e604039b92742/raw/DSH_NEWS.md',
      theme: 'system',
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
const taskProgressListeners = new Set<Listener<TaskProgress>>()
const taskLogListeners = new Set<Listener<TaskLog>>()

function emitStatus(s: InstanceStatus) {
  statusListeners.forEach((fn) => fn(s))
}
function emitTaskProgress(p: TaskProgress) {
  taskProgressListeners.forEach((fn) => fn(p))
}
function emitTaskLog(l: TaskLog) {
  taskLogListeners.forEach((fn) => fn(l))
}

// Mock task storage (runtime only, like the real backend).
const mockTasks = new Map<string, TaskInfo>()
// Mock profiles created at runtime per home id.
const mockProfiles: Record<string, string[]> = {}

function mockNewId(prefix: string): string {
  return `${prefix}-${uuid()}`
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
      return `C:\\Users\\Administrator\\AppData\\Roaming\\in.dsh-plug.dsh-launcher\\homes\\${safe}` as T
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
    case 'start_create_instance_task': {
      const name = String(args?.name ?? '').trim()
      const version = String(args?.version ?? '').trim()
      const dedicated = Boolean(args?.dedicated)
      const homeIdArg = args?.home_id as string | null
      if (!name) fail('实例名称不能为空')
      if (!version) fail('版本号不能为空')
      if (db.instances.some((i) => i.name === name)) fail('同名实例已存在')
      if ([...mockTasks.values()].some((t) => t.state === 'running' && t.instance_name === name)) {
        fail('同名实例的下载任务已在进行中')
      }

      // Dedicated HOME is only materialized when the task finishes, mirroring
      // the real backend's placeholder semantics.
      let dedicatedPath: string | null = null
      if (dedicated) {
        const safe = name.replace(/[^\w一-龥.-]+/g, '_')
        dedicatedPath = `C:\\Users\\Administrator\\AppData\\Roaming\\in.dsh-plug.dsh-launcher\\homes\\${safe}`
      }
      if (!dedicated && (!homeIdArg || !db.homes.some((h) => h.id === homeIdArg))) {
        fail('请选择 DSH_HOME')
      }

      const task: TaskInfo = {
        id: mockNewId('t'),
        kind: 'create-instance',
        label: `下载 DSH ${version} 并创建实例「${name}」`,
        version,
        state: 'running',
        percent: 0,
        created_at: Date.now(),
        message: null,
        instance_id: null,
        instance_name: name,
        logs: [],
      }
      mockTasks.set(task.id, task)
      emitTaskProgress({ id: task.id, state: 'running', percent: 0, message: null, instance_id: null })

      // Simulate npm --loglevel=http download + install + instance creation.
      const fakeLogs = [
        `npm http fetch GET 200 https://registry.npmjs.org/@deepseek-ai%2fdsh 120ms`,
        `npm http fetch GET 200 https://registry.npmjs.org/@deepseek-ai/dsh/-/dsh-${version}.tgz 480ms`,
        `npm info ok`,
        `added 213 packages in 2s`,
      ]
      let step = 0
      const timer = setInterval(() => {
        const t = mockTasks.get(task.id)
        if (!t || t.state !== 'running') {
          clearInterval(timer)
          return
        }
        if (step < fakeLogs.length) {
          const line = fakeLogs[step]
          t.logs.push(line)
          t.percent = Math.min(95, t.percent + 20)
          emitTaskLog({ id: task.id, line })
          emitTaskProgress({ id: task.id, state: 'running', percent: t.percent, message: null, instance_id: null })
          step += 1
          return
        }
        clearInterval(timer)
        const cur = loadDb()
        // Resolve the HOME now (dedicated HOME is created at completion time).
        let resolvedHomeId = homeIdArg
        if (dedicated && dedicatedPath) {
          const existing = cur.homes.find(
            (h) => h.path.replace(/\\/g, '/').toLowerCase() === dedicatedPath!.replace(/\\/g, '/').toLowerCase(),
          )
          if (existing) {
            resolvedHomeId = existing.id
          } else {
            const home: DshHome = { id: mockNewId('h'), name, path: dedicatedPath! }
            cur.homes.push(home)
            resolvedHomeId = home.id
          }
        }
        // Install version record if missing.
        let ver = cur.versions.find((v) => v.version === version)
        if (!ver) {
          ver = {
            id: mockNewId('v'),
            version,
            dir: `C:\\Users\\Administrator\\AppData\\Roaming\\in.dsh-plug.dsh-launcher\\versions\\${version}`,
          }
          cur.versions.push(ver)
        }
        const inst: DshInstance = {
          id: mockNewId('i'),
          name,
          version_id: ver.id,
          home_id: resolvedHomeId!,
          env_overrides: {},
          default_profile: null,
          last_profile: null,
        }
        cur.instances.push(inst)
        saveDb(cur)
        const doneTask = mockTasks.get(task.id)
        if (doneTask) {
          doneTask.state = 'done'
          doneTask.percent = 100
          doneTask.instance_id = inst.id
        }
        emitTaskProgress({ id: task.id, state: 'done', percent: 100, message: null, instance_id: inst.id })
      }, 600)

      return task.id as T
    }
    case 'get_runtime_status': {
      // Browser preview: assume Node + pnpm are available so the UI is usable.
      const mockRuntime: RuntimeStatus = {
        node: { installed: true, version: 'v22.14.0', path: null },
        pnpm: { installed: true, version: '9.15.4', path: null },
      }
      return mockRuntime as T
    }
    case 'list_tasks': {
      return [...mockTasks.values()].sort((a, b) => b.created_at - a.created_at) as T
    }
    case 'remove_task': {
      const id = String(args?.id)
      const t = mockTasks.get(id)
      if (!t) fail('任务不存在')
      if (t.state === 'running') fail('任务仍在运行，请先取消')
      mockTasks.delete(id)
      return undefined as T
    }
    case 'cancel_task': {
      const id = String(args?.id)
      const t = mockTasks.get(id)
      if (!t) fail('任务不存在')
      t.state = 'cancelled'
      t.message = '已取消'
      emitTaskProgress({ id, state: 'cancelled', percent: t.percent, message: '已取消', instance_id: null })
      return undefined as T
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
      // Mock: combine the default set with previously created profiles.
      const base: string[] = home.path.endsWith('.dsh') ? ['web', 'demo', 'pack'] : ['web']
      const extras = mockProfiles[homeId] ?? []
      return [...base, ...extras] as T
    }
    case 'create_profile': {
      const homeId = String(args?.home_id)
      const name = String(args?.name ?? '').trim()
      if (!name) fail('Profile 名称不能为空')
      if (name === '__temp__' || name === 'node_modules') fail(`「${name}」为保留名称，不能使用`)
      if (!/^[A-Za-z0-9._-]+$/.test(name)) fail('Profile 名称只能包含字母、数字、-、_、.')
      const base: string[] = []
      const home = db.homes.find((h) => h.id === homeId)
      if (home && home.path.endsWith('.dsh')) base.push(...['web', 'demo', 'pack'])
      mockProfiles[homeId] = mockProfiles[homeId] ?? []
      if (base.includes(name) || mockProfiles[homeId].includes(name)) fail(`Profile「${name}」已存在`)
      mockProfiles[homeId].push(name)
      return name as T
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
      // Browser preview has no Tauri webview windows; deliberately do NOT
      // window.open here so the browser never navigates to a profile page.
      return undefined as T
    }
    case 'list_instance_status':
      return Object.values(db.running) as T
    case 'get_settings':
      return db.settings as T
    case 'fetch_news': {
      // Browser preview: return sample markdown instead of fetching.
      return [
        '# DSH Launcher 新闻',
        '',
        '- 支持 **GFM** 表格、任务列表与`行内代码`',
        '- 支持 <b>内联 HTML</b>（已过滤 XSS）',
        '',
        '| 版本 | 状态 |',
        '| ---- | ---- |',
        '| rc.6 | ✅   |',
      ].join('\n') as T
    }
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

  getRuntimeStatus: () => call<RuntimeStatus>('get_runtime_status'),

  listHomes: () => call<DshHome[]>('list_homes'),
  createHome: (name: string, path: string) => call<DshHome>('create_home', { name, path }),
  removeHome: (id: string) => call<void>('remove_home', { id }),
  defaultDedicatedHomePath: (name: string) => call<string>('default_dedicated_home_path', { name }),

  listVersions: () => call<DshVersion[]>('list_versions'),
  fetchAvailableVersions: () => call<RemoteVersion[]>('fetch_available_versions'),
  removeVersion: (id: string) => call<void>('remove_version', { id }),

  startCreateInstanceTask: (name: string, version: string, homeId: string | null, dedicated: boolean) =>
    call<string>('start_create_instance_task', { name, version, home_id: homeId, dedicated }),
  listTasks: () => call<TaskInfo[]>('list_tasks'),
  removeTask: (id: string) => call<void>('remove_task', { id }),
  cancelTask: (id: string) => call<void>('cancel_task', { id }),

  listInstances: () => call<DshInstance[]>('list_instances'),
  createInstance: (input: NewInstanceInput) => call<DshInstance>('create_instance', { input }),
  updateInstance: (input: DshInstance) => call<DshInstance>('update_instance', { input }),
  deleteInstance: (id: string) => call<void>('delete_instance', { id }),

  listProfiles: (homeId: string) => call<string[]>('list_profiles', { home_id: homeId }),
  createProfile: (homeId: string, name: string) =>
    call<string>('create_profile', { home_id: homeId, name }),

  startInstance: (id: string, profile: string) => call<void>('start_instance', { id, profile }),
  stopInstance: (id: string) => call<void>('stop_instance', { id }),
  openInstanceWindow: (id: string) => call<void>('open_instance_window', { id }),
  listInstanceStatus: () => call<InstanceStatus[]>('list_instance_status'),

  getSettings: () => call<LauncherSettings>('get_settings'),
  updateSettings: (settings: Partial<LauncherSettings>) => call<LauncherSettings>('update_settings', { settings }),
  fetchNews: (source: string) => call<string>('fetch_news', { source }),

  async onInstanceStatus(cb: Listener<InstanceStatus>): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event')
      const un = await listen<InstanceStatus>('instance://status', (e) => cb(e.payload))
      return un
    }
    statusListeners.add(cb)
    return () => statusListeners.delete(cb)
  },

  async onTaskProgress(cb: Listener<TaskProgress>): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event')
      const un = await listen<TaskProgress>('task://progress', (e) => cb(e.payload))
      return un
    }
    taskProgressListeners.add(cb)
    return () => taskProgressListeners.delete(cb)
  },

  async onTaskLog(cb: Listener<TaskLog>): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import('@tauri-apps/api/event')
      const un = await listen<TaskLog>('task://log', (e) => cb(e.payload))
      return un
    }
    taskLogListeners.add(cb)
    return () => taskLogListeners.delete(cb)
  },
}
