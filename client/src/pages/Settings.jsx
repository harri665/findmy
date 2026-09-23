import { useEffect, useState } from 'react'
import { api } from '../api.js'

const input = 'mt-1 w-full rounded-md border border-gray-300 px-3 py-2 focus:border-gray-900 focus:outline-none'

export default function Settings() {
  const [form, setForm] = useState(null)
  const [status, setStatus] = useState(null)
  const [error, setError] = useState(null)

  useEffect(() => {
    api
      .settings()
      .then((s) => setForm({ anisette_url: s.anisette_url ?? '', fetch_interval_min: s.fetch_interval_min ?? 15 }))
      .catch((e) => setError(e.message))
  }, [])

  async function save(e) {
    e.preventDefault()
    setError(null)
    setStatus(null)
    try {
      await api.saveSettings({
        anisette_url: form.anisette_url.trim() || null,
        fetch_interval_min: Number(form.fetch_interval_min),
      })
      setStatus('Saved')
    } catch (err) {
      setError(err.message)
    }
  }

  if (!form) return <p className="page text-gray-500">{error ?? 'Loading…'}</p>

  return (
    <div className="page max-w-2xl space-y-4">
      <h1 className="text-2xl font-bold">Settings</h1>
      <form onSubmit={save} className="space-y-4 rounded-xl border border-gray-200 bg-white p-5">
        <h2 className="font-semibold">External report fetcher</h2>
        <label className="block">
          <span className="text-sm text-gray-700">Anisette provider URL</span>
          <input
            className={input}
            placeholder="http://anisette:6969"
            value={form.anisette_url}
            onChange={(e) => setForm({ ...form, anisette_url: e.target.value })}
          />
          <span className="mt-1 block text-xs text-gray-500">
            Apple requires these headers on report requests. findmy doesn't include a provider; choosing and running one is up to
            you.
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
        <p className="rounded-md bg-amber-50 p-3 text-sm text-amber-800">
          These values are only stored for an external fetcher to read; the server doesn't fetch reports itself. A fetcher gets
          the IDs to request from <code>GET /api/keys/hashed</code> and pushes what it finds to <code>POST /api/reports</code>.
        </p>
        {error && <p className="text-sm text-red-600">{error}</p>}
        {status && <p className="text-sm text-green-700">{status}</p>}
        <button className="rounded-md bg-gray-900 px-4 py-2 font-medium text-white hover:bg-gray-700">Save</button>
      </form>
    </div>
  )
}
