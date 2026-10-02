import { useEffect, useState } from 'react'
import { api } from '../api.js'
import { dateTime } from '../lib/format.js'

const input = 'mt-1 w-full rounded-md border border-gray-300 px-3 py-2 focus:border-gray-900 focus:outline-none'

export default function Settings() {
  const [form, setForm] = useState(null)
  const [status, setStatus] = useState(null)
  const [error, setError] = useState(null)
  const [apple, setApple] = useState(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let cancelled = false
    Promise.all([api.settings(), api.appleStatus()])
      .then(([s, a]) => {
        if (cancelled) return
        setForm({ anisette_url: s.anisette_url ?? '', fetch_interval_min: s.fetch_interval_min ?? 15 })
        setApple(a)
      }).catch((e) => !cancelled && setError(e.message))
    const timer = setInterval(() => {
      api.appleStatus().then((a) => !cancelled && setApple(a)).catch((e) => !cancelled && setError(e.message))
    }, 5000)
    return () => { cancelled = true; clearInterval(timer) }
  }, [])

  async function save(e) {
    e.preventDefault()
    setError(null)
    setStatus(null)
    setBusy(true)
    try {
      await api.saveSettings({
        anisette_url: form.anisette_url.trim() || null,
        fetch_interval_min: Number(form.fetch_interval_min),
      })
      setStatus('Saved')
      setApple(await api.appleStatus())
    } catch (err) {
      setError(err.message)
    } finally {
      setBusy(false)
    }
  }

  async function importSession(e) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      if (file.size > 256 * 1024) throw new Error('Session file is too large (maximum 256 KB)')
      const session = JSON.parse(await file.text())
      setApple(await api.importAppleSession(session))
      setStatus('Apple session imported')
    } catch (err) {
      setError(err instanceof SyntaxError ? 'Choose a valid JSON session file' : err.message)
    } finally {
      setBusy(false)
    }
  }

  async function fetchNow() {
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      const result = await api.fetchReports()
      setStatus(`Fetched reports: ${result.stored} new, ${result.duplicate} already stored, ${result.invalid} invalid`)
    } catch (err) {
      setError(err.message)
    } finally {
      try { setApple(await api.appleStatus()) } catch { /* Keep the fetch error visible. */ }
      setBusy(false)
    }
  }

  async function disconnect() {
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      await api.disconnectApple()
      setApple(await api.appleStatus())
      setStatus('Apple session removed')
    } catch (err) {
      setError(err.message)
    } finally {
      setBusy(false)
    }
  }

  if (!form) return <p className="page text-gray-500">{error ?? 'Loading…'}</p>

  return (
    <div className="page max-w-2xl space-y-4">
      <h1 className="text-2xl font-bold">Settings</h1>
      <form onSubmit={save} className="space-y-4 rounded-xl border border-gray-200 bg-white p-5">
        <h2 className="font-semibold">Report fetching</h2>
        <label className="block">
          <span className="text-sm text-gray-700">Anisette provider URL</span>
          <input
            className={input}
            placeholder="http://anisette:6969"
            value={form.anisette_url}
            onChange={(e) => setForm({ ...form, anisette_url: e.target.value })}
          />
          <span className="mt-1 block text-xs text-gray-500">
            Enter the JSON header endpoint of your anisette provider. The server must be able to reach this URL.
          </span>
        </label>
        <label className="block">
          <span className="text-sm text-gray-700">Fetch every (minutes)</span>
          <input
            type="number"
            min={5}
            max={1440}
            className={input}
            value={form.fetch_interval_min}
            onChange={(e) => setForm({ ...form, fetch_interval_min: e.target.value })}
          />
        </label>
        <button disabled={busy} className="rounded-md bg-gray-900 px-4 py-2 font-medium text-white hover:bg-gray-700 disabled:opacity-50">Save</button>
      </form>
      <section className="space-y-3 rounded-xl border border-gray-200 bg-white p-5">
        <h2 className="font-semibold">Apple session</h2>
        <p className="text-sm text-gray-600">
          Sign in with the project's login helper, then import its session JSON file here.
          Your session is encrypted on the server. Sign in again and replace it if Apple expires the session.
        </p>
        <details className="text-sm text-gray-600">
          <summary className="cursor-pointer font-medium text-gray-800">How to create a session file</summary>
          <p className="mt-2">From a checkout of this project, install the helper's dependencies and run it with your provider URL:</p>
          <pre className="mt-2 overflow-x-auto rounded-md bg-gray-900 p-3 text-xs text-gray-100">{'python -m pip install -r tools/apple/requirements.txt\npython tools/apple/login.py --anisette-url http://your-provider:6969'}</pre>
          <p className="mt-2">Complete the password and verification prompts, then choose the generated findmy.apple-session.json file below. The login machine and server must use the same provider.</p>
        </details>
        <label className="block text-sm font-medium">
          Import session JSON
          <input type="file" accept=".json,application/json" disabled={busy || apple?.running} onChange={importSession} className="mt-1 block w-full text-sm" />
        </label>
        <p className="text-sm">{apple?.connected ? 'Apple session saved' : 'No Apple session saved'}</p>
        {apple?.last_success && <p className="text-sm text-gray-600">Last successful fetch: {dateTime(apple.last_success)}</p>}
        {apple?.configured && !apple.running && <p className="text-sm text-gray-600">Next automatic fetch: {dateTime(apple.next_fetch)}</p>}
        {apple?.running && <p role="status" className="text-sm text-gray-600">Fetching reports…</p>}
        {apple?.error && <p role="alert" className="text-sm text-red-600">Last fetch failed: {apple.error}</p>}
        {apple?.result && <p className="text-sm text-gray-600">Last fetch: {apple.result.stored} new reports, {apple.result.duplicate} already stored, {apple.result.invalid} invalid.</p>}
        <div className="flex gap-3">
          <button onClick={fetchNow} disabled={busy || apple?.running || !apple?.configured} className="rounded-md bg-gray-900 px-4 py-2 text-sm font-medium text-white disabled:opacity-50">Fetch now</button>
          {apple?.connected && <button onClick={disconnect} disabled={busy || apple.running} className="text-sm text-red-700 disabled:opacity-50">Remove session</button>}
        </div>
      </section>
      {error && <p role="alert" className="text-sm text-red-600">{error}</p>}
      {status && <p role="status" className="text-sm text-green-700">{status}</p>}
    </div>
  )
}
