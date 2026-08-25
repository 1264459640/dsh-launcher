import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  // Hash history keeps the Tauri webview happy without server rewrites.
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'home', component: () => import('@/views/Home.vue') },
    { path: '/instances', name: 'instances', component: () => import('@/views/Instances.vue') },
    { path: '/versions', name: 'versions', component: () => import('@/views/Versions.vue') },
    { path: '/plugins', name: 'plugins', component: () => import('@/views/Plugins.vue') },
    { path: '/settings', name: 'settings', component: () => import('@/views/Settings.vue') },
    { path: '/instances/new', name: 'instance-new', component: () => import('@/views/InstanceEdit.vue') },
    { path: '/instances/:id', name: 'instance-edit', component: () => import('@/views/InstanceEdit.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router
