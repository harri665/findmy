// Thin wrapper over the fm-server API (/api, proxied to :3001).

const TOKEN_KEY = 'findmy.token'

export function getToken() {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export function setToken(token) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token)
    else localStorage.removeItem(TOKEN_KEY)
  } catch {
    // Storage unavailable (private mode): the session lasts until reload.
  }
}

export class ApiError extends Error {
  constructor(status, message) {
    super(message)
    this.status = status
  }
}

let onUnauthorized = () => {}
export function setUnauthorizedHandler(fn) {
  onUnauthorized = fn
}

async function request(method, path, { body, raw } = {}) {
  const headers = {}
  const token = getToken()
  if (token) headers.Authorization = `Bearer ${token}`
  if (body !== undefined) headers['Content-Type'] = 'application/json'

  const res = await fetch(`/api${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (res.status === 401 && path !== '/login') {
    setToken(null)
    onUnauthorized()
  }
  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`
    try {
      message = (await res.json()).error ?? message
    } catch {
      // not JSON
    }
    throw new ApiError(res.status, message)
  }
  if (raw) return res
  if (res.status === 204) return null
  return res.json()
}

export const api = {
  health: () => request('GET', '/health'),
  login: (password) => request('POST', '/login', { body: { password } }),
  logout: () => request('POST', '/logout'),

  devices: () => request('GET', '/devices'),
  device: (id) => request('GET', `/devices/${id}`),
  createDevice: (body) => request('POST', '/devices', { body }),
  importDevice: (body) => request('POST', '/devices/import', { body }),
  updateDevice: (id, body) => request('PATCH', `/devices/${id}`, { body }),
  deleteDevice: (id) => request('DELETE', `/devices/${id}`),
  rotateAgentToken: (id) => request('POST', `/devices/${id}/agent-token`),
  locations: (id, since) => request('GET', `/devices/${id}/locations${since != null ? `?since=${since}` : ''}`),

  settings: () => request('GET', '/settings'),
  saveSettings: (body) => request('PUT', '/settings', { body }),

  async downloadBlob(id, filename) {
    const res = await request('GET', `/devices/${id}/blob`, { raw: true })
    const url = URL.createObjectURL(await res.blob())
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
  },
}
