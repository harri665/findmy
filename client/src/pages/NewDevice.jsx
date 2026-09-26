import { useState } from 'react'
import { Link } from 'react-router-dom'
import { api } from '../api.js'
import Setup from '../components/Setup.jsx'
import { ICONS, PLATFORMS, duration } from '../lib/format.js'

const input = 'mt-1 w-full rounded-md border border-gray-300 px-3 py-2 focus:border-gray-900 focus:outline-none'

export default function NewDevice() {
  const [tab, setTab] = useState('create')
  const [created, setCreated] = useState(null)

  if (created) {
    return (
      <div className="page max-w-2xl space-y-4">
        <h1 className="text-2xl font-bold">
          {created.device.icon} {created.device.name} is ready
        </h1>
        <p className="text-gray-600">
          {created.device.key_count} key{created.device.key_count === 1 ? '' : 's'} generated. Private keys stay on this server,
          encrypted. The beacon only ever gets public keys.
        </p>
        <Setup device={created.device} agentToken={created.agent_token} />
        <Link to={`/devices/${created.device.id}`} className="inline-block text-sm font-medium hover:underline">
          Go to device →
        </Link>
      </div>
    )
  }

  return (
    <div className="page max-w-2xl space-y-4">
      <h1 className="text-2xl font-bold">Add device</h1>
      <div className="flex gap-1 text-sm">
        {[
          ['create', 'New keys'],
          ['import', 'Import key file'],
        ].map(([id, label]) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`rounded-md px-3 py-1.5 font-medium ${tab === id ? 'bg-gray-900 text-white' : 'bg-gray-200 text-gray-700'}`}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === 'create' ? <CreateForm onCreated={setCreated} /> : <ImportForm onCreated={setCreated} />}
    </div>
  )
}

function CommonFields({ form, set }) {
  return (
    <>
      <label className="block">
        <span className="text-sm text-gray-700">Name</span>
        <input className={input} value={form.name} maxLength={64} onChange={(e) => set({ name: e.target.value })} required />
      </label>
      <div>
        <span className="text-sm text-gray-700">Icon</span>
        <div className="mt-1 flex flex-wrap gap-1">
          {ICONS.map((i) => (
            <button
              type="button"
              key={i}
              onClick={() => set({ icon: i })}
              className={`rounded-md border px-2 py-1 text-xl ${form.icon === i ? 'border-gray-900 bg-gray-100' : 'border-gray-200'}`}
            >
              {i}
            </button>
          ))}
        </div>
      </div>
      <label className="block">
        <span className="text-sm text-gray-700">Runs on</span>
        <select className={input} value={form.platform} onChange={(e) => set({ platform: e.target.value })}>
          {PLATFORMS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </label>
    </>
  )
}

function CreateForm({ onCreated }) {
  const [form, setForm] = useState({ name: '', icon: ICONS[0], platform: 'esp32', key_mode: 'pool', count: 96, interval_min: 15 })
  const [error, setError] = useState(null)
  const [busy, setBusy] = useState(false)
  const set = (patch) =>
    setForm((f) => {
      const next = { ...f, ...patch }
      // ESP32 tags can't derive keys on-device.
      if (next.platform === 'esp32' && next.key_mode === 'derived') next.key_mode = 'pool'
      return next
    })

  async function submit(e) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onCreated(
        await api.createDevice({
          name: form.name,
          icon: form.icon,
          platform: form.platform,
          key_mode: form.key_mode,
          count: Number(form.count),
          interval_s: Number(form.interval_min) * 60,
        }),
      )
    } catch (err) {
      setError(err.message)
    } finally {
      setBusy(false)
    }
  }

  const cycle = form.count * form.interval_min * 60
  const noDerive = form.platform === 'esp32'

  return (
    <form onSubmit={submit} className="space-y-4 rounded-xl border border-gray-200 bg-white p-5">
      <CommonFields form={form} set={set} />

      <fieldset className="space-y-2">
        <legend className="text-sm text-gray-700">Keys</legend>
        <label className="flex items-start gap-2">
          <input type="radio" checked={form.key_mode === 'pool'} onChange={() => set({ key_mode: 'pool' })} className="mt-1" />
          <span>
            <span className="font-medium">Rotating pool</span>
            <span className="block text-sm text-gray-600">
              Switches keys on a timer, like an AirTag, so strangers can't follow one key around.
            </span>
          </span>
        </label>
        <label className={`flex items-start gap-2 ${noDerive ? 'opacity-50' : ''}`}>
          <input
            type="radio"
            checked={form.key_mode === 'derived'}
            disabled={noDerive}
            onChange={() => set({ key_mode: 'derived' })}
            className="mt-1"
          />
          <span>
            <span className="font-medium">Derived from one seed</span>
            <span className="block text-sm text-gray-600">
              Like a rotating pool, but the beacon stores a single secret seed. {noDerive && 'Not available on ESP32 tags.'}
            </span>
          </span>
        </label>
        <label className="flex items-start gap-2">
          <input type="radio" checked={form.key_mode === 'static'} onChange={() => set({ key_mode: 'static' })} className="mt-1" />
          <span>
            <span className="font-medium">Single static key</span>
            <span className="block text-sm text-gray-600">Simplest, but anyone recording the beacon can track it over time.</span>
          </span>
        </label>
      </fieldset>

      {form.key_mode !== 'static' && (
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="text-sm text-gray-700">{form.key_mode === 'derived' ? 'Keys in window' : 'Keys in pool'}</span>
            <input type="number" min={2} max={2048} className={input} value={form.count} onChange={(e) => set({ count: e.target.value })} />
          </label>
          <label className="block">
            <span className="text-sm text-gray-700">Rotate every (minutes)</span>
            <input
              type="number"
              min={1}
              max={1440}
              className={input}
              value={form.interval_min}
              onChange={(e) => set({ interval_min: e.target.value })}
            />
          </label>
          <p className="col-span-2 text-sm text-gray-600">Keys repeat every {duration(cycle)}.</p>
        </div>
      )}

      {error && <p className="text-sm text-red-600">{error}</p>}
      <button disabled={busy || !form.name.trim()} className="rounded-md bg-gray-900 px-4 py-2 font-medium text-white disabled:opacity-50">
        {busy ? 'Generating keys…' : 'Create device'}
      </button>
    </form>
  )
}

function ImportForm({ onCreated }) {
  const [form, setForm] = useState({ name: '', icon: ICONS[0], platform: 'esp32' })
  const [keyFile, setKeyFile] = useState(null)
  const [error, setError] = useState(null)
  const [busy, setBusy] = useState(false)
  const set = (patch) => setForm((f) => ({ ...f, ...patch }))

  async function pickFile(e) {
    setError(null)
    const file = e.target.files?.[0]
    if (!file) return
    try {
      const parsed = JSON.parse(await file.text())
      if (!Array.isArray(parsed.keys) || !parsed.mode) throw new Error('not an fm key file')
      setKeyFile(parsed)
      if (!form.name) set({ name: parsed.name ?? '' })
    } catch (err) {
      setKeyFile(null)
      setError(`Couldn't read ${file.name}: ${err.message}`)
    }
  }

  async function submit(e) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onCreated(await api.importDevice({ ...keyFile, ...form }))
    } catch (err) {
      setError(err.message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <form onSubmit={submit} className="space-y-4 rounded-xl border border-gray-200 bg-white p-5">
      <p className="text-sm text-gray-600">
        Import keys made with <code>fm keygen</code>. Once imported, delete the .keys.json file or keep it somewhere safe; it
        contains private keys.
      </p>
      <input type="file" accept=".json,application/json" onChange={pickFile} className="block text-sm" />
      {keyFile && (
        <p className="text-sm text-gray-700">
          {keyFile.keys.length} key{keyFile.keys.length === 1 ? '' : 's'}, {keyFile.mode} mode
          {keyFile.mode !== 'static' && `, rotating every ${duration(keyFile.interval_s)}`}
        </p>
      )}
      <CommonFields form={form} set={set} />
      {error && <p className="text-sm text-red-600">{error}</p>}
      <button disabled={busy || !keyFile || !form.name.trim()} className="rounded-md bg-gray-900 px-4 py-2 font-medium text-white disabled:opacity-50">
        {busy ? 'Importing…' : 'Import device'}
      </button>
    </form>
  )
}
