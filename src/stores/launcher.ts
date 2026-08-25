import { defineStore } from 'pinia'
import { Message } from '@arco-design/web-vue'
import { api } from '@/api'
import type {
  DshHome,
  DshInstance,
  DshVersion,
  InstanceStatus,
  LauncherSettings,
  RemoteVersion,
  TaskInfo,
} from '@/api/types'

interface LauncherState {
  homes: DshHome[]
  versions: DshVersion[]
  instances: DshInstance[]
  settings: LauncherSettings
  statusById: Record<string, InstanceStatus>
  tasks: Record<string, TaskInfo>
  remoteVersions: RemoteVersion[]
  remoteLoading: boolean
  loaded: boolean
}

export const useLauncherStore = defineStore('launcher', {
  state: (): LauncherState => ({
    homes: [],
    versions: [],
    instances: [],
    settings: {
      locale: 'zh-CN',
      minimize_to_tray: true,
      autostart: false,
      last_instance_id: null,
    },
    statusById: {},
    tasks: {},
    remoteVersions: [],
    remoteLoading: false,
    loaded: false,
  }),

  getters: {
    versionById: (s) => (id: string) => s.versions.find((v) => v.id === id),
    homeById: (s) => (id: string) => s.homes.find((h) => h.id === id),
    instanceById: (s) => (id: string) => s.instances.find((i) => i.id === id),
    statusOf: (s) => (id: string): InstanceStatus =>
      s.statusById[id] ?? { id, state: 'stopped', url: null, profile: null, exit_code: null },
    taskList: (s) => Object.values(s.tasks).sort((a, b) => b.created_at - a.created_at),
    runningTaskCount: (s) => Object.values(s.tasks).filter((t) => t.state === 'running').length,
  },

  actions: {
    async init() {
      const [homes, versions, instances, settings, statuses, tasks] = await Promise.all([
        api.listHomes(),
        api.listVersions(),
        api.listInstances(),
        api.getSettings(),
        api.listInstanceStatus(),
        api.listTasks(),
      ])
      this.homes = homes
      this.versions = versions
      this.instances = instances
      this.settings = settings
      this.statusById = Object.fromEntries(statuses.map((st) => [st.id, st]))
      this.tasks = Object.fromEntries(tasks.map((t) => [t.id, t]))
      this.loaded = true

      await api.onInstanceStatus((st) => {
        if (st.state === 'stopped' || st.state === 'exited') {
          delete this.statusById[st.id]
        } else {
          this.statusById[st.id] = st
        }
      })

      await api.onTaskProgress((p) => {
        const existing = this.tasks[p.id]
        if (existing) {
          existing.state = p.state
          existing.percent = p.percent
          existing.message = p.message
          existing.instance_id = p.instance_id
        }
        // A create-instance task finished: refresh instance/version lists.
        if (p.state === 'done' && p.instance_id) {
          this.refreshInstances()
          this.refreshVersions()
          this.refreshHomes()
        }
      })

      await api.onTaskLog((l) => {
        const existing = this.tasks[l.id]
        if (!existing) return
        if (existing.logs.length >= 1000) existing.logs.shift()
        existing.logs.push(l.line)
      })
    },

    async refreshInstances() {
      this.instances = await api.listInstances()
    },
    async refreshVersions() {
      this.versions = await api.listVersions()
    },
    async refreshHomes() {
      this.homes = await api.listHomes()
    },
    async refreshSettings() {
      this.settings = await api.getSettings()
    },
    async refreshTasks() {
      const tasks = await api.listTasks()
      this.tasks = Object.fromEntries(tasks.map((t) => [t.id, t]))
    },

    async refreshRemoteVersions() {
      this.remoteLoading = true
      try {
        this.remoteVersions = await api.fetchAvailableVersions()
      } catch (e) {
        Message.error(String(e))
      } finally {
        this.remoteLoading = false
      }
    },
  },
})
