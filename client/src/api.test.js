import { afterEach, describe, expect, it, vi } from 'vitest'
import { api, getToken, setToken, setUnauthorizedHandler } from './api.js'

afterEach(() => {
  setToken(null)
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('API session handling', () => {
  it('keeps an in-memory session when browser storage is unavailable', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked') })
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked') })
    setToken('session')
    expect(getToken()).toBe('session')
  })

  it('clears expired sessions and notifies the app', async () => {
    setToken('session')
    const unauthorized = vi.fn()
    setUnauthorizedHandler(unauthorized)
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ status: 401, ok: false, json: async () => ({ error: 'unauthorized' }) }))
    await expect(api.appleStatus()).rejects.toThrow('unauthorized')
    expect(getToken()).toBeNull()
    expect(unauthorized).toHaveBeenCalledOnce()
  })

  it('sends the session only to the authenticated API', async () => {
    setToken('session')
    const fetch = vi.fn().mockResolvedValue({ status: 200, ok: true, json: async () => ({ stored: 1 }) })
    vi.stubGlobal('fetch', fetch)
    await api.fetchReports()
    expect(fetch).toHaveBeenCalledWith('/api/reports/fetch', expect.objectContaining({ method: 'POST', headers: { Authorization: 'Bearer session' } }))
  })
})
