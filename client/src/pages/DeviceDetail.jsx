import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { api } from '../api.js'
import DeviceMap from '../components/DeviceMap.jsx'
import Setup from '../components/Setup.jsx'
import { PLATFORMS, battery, coords, dateTime, duration, timeAgo } from '../lib/format.js'

const RANGES = [
  ['24 h', 86400],
  ['3 days', 3 * 86400],
  ['7 days', 7 * 86400],
]

export default function DeviceDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const [device, setDevice] = useState(null)
  const [history, setHistory] = useState([])
  const [range, setRange] = useState(RANGES[0][1])
  const [error, setError] = useState(null)
  const [agentToken, setAgentToken] = useState(null)
  const [renaming, setRenaming] = useState(false)
  const [name, setName] = useState('')

  useEffect(() => {
    api
      .device(id)
      .then((d) => {
        setDevice(d)
        setName(d.name)
      })
      .catch((e) => setError(e.status === 404 ? 'Device not found' : e.message))
  }, [id])

  useEffect(() => {
    const since = Math.floor(Date.now() / 1000) - range
    api.locations(id, since).then(setHistory).catch((e) => setError(e.message))
  }, [id, range])

  async function rename(e) {
    e.preventDefault()
    try {
      setDevice(await api.updateDevice(id, { name }))
      setRenaming(false)
    } catch (err) {
      setError(err.message)
    }
  }

  async function newToken() {
    if (!window.confirm('Generate a new agent token? The old one stops working immediately.')) return
    try {
      setAgentToken((await api.rotateAgentToken(id)).agent_token)
    } catch (err) {
      setError(err.message)
    }
  }

  async function remove() {
    if (!window.confirm(`Delete ${device.name}? Its keys and location history are deleted permanently.`)) return
    try {
      await api.deleteDevice(id)
      navigate('/devices')
    } catch (err) {
      setError(err.message)
    }
  }

  if (error && !device) return <p className="page text-red-600">{error}</p>
  if (!device) return <p className="page text-gray-500">Loading…</p>

  const platform = PLATFORMS.find((p) => p.id === device.platform)?.label ?? device.platform

  return (
    <div className="page space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          {renaming ? (
            <form onSubmit={rename} className="flex gap-2">
              <input value={name} maxLength={64} onChange={(e) => setName(e.target.value)} className="rounded-md border border-gray-300 px-2 py-1 text-xl" autoFocus />
              <button className="rounded-md bg-gray-900 px-3 text-sm text-white">Save</button>
              <button type="button" onClick={() => setRenaming(false)} className="text-sm text-gray-600">
                Cancel
              </button>
            </form>
          ) : (
            <h1 className="text-2xl font-bold">
              {device.icon} {device.name}{' '}
              <button onClick={() => setRenaming(true)} className="align-middle text-sm font-normal text-gray-500 hover:text-gray-900">
                rename
              </button>
            </h1>
          )}
          <p className="text-sm text-gray-600">
            {platform} · {device.key_count} {device.key_mode} key{device.key_count === 1 ? '' : 's'}
            {device.key_mode === 'pool' && `, rotating every ${duration(device.interval_s)}`} · {device.report_count} reports
          </p>
        </div>
        {device.last && (
          <div className="text-right text-sm">
            <p className="font-medium">Last seen {timeAgo(device.last.ts)}</p>
            <p className="text-gray-600">
              {coords(device.last.lat, device.last.lon)} · ±{device.last.accuracy} m · battery {battery(device.last.status)}
            </p>
          </div>
        )}
      </div>

      {error && <p className="text-sm text-red-600">{error}</p>}

      <section className="space-y-2">
        <div className="flex items-center gap-2">
          <h2 className="font-semibold">History</h2>
          {RANGES.map(([label, secs]) => (
            <button
              key={secs}
              onClick={() => setRange(secs)}
              className={`rounded-md px-2 py-0.5 text-sm ${range === secs ? 'bg-gray-900 text-white' : 'bg-gray-200 text-gray-700'}`}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="h-96">
          <DeviceMap devices={[device]} history={history.length ? history : []} />
        </div>
        {history.length === 0 ? (
          <p className="text-sm text-gray-600">
            No reports in this period. Reports appear after Apple devices pass the beacon and the server fetches them.
          </p>
        ) : (
          <details className="text-sm">
            <summary className="cursor-pointer text-gray-700">{history.length} reports</summary>
            <ul className="mt-2 max-h-64 overflow-y-auto rounded-md border border-gray-200 bg-white">
              {[...history].reverse().map((p, i) => (
                <li key={`${p.ts}-${i}`} className="flex justify-between border-b border-gray-100 px-3 py-1.5">
                  <span>{dateTime(p.ts)}</span>
                  <a className="text-blue-700 hover:underline" target="_blank" rel="noreferrer" href={`https://www.openstreetmap.org/?mlat=${p.lat}&mlon=${p.lon}#map=17/${p.lat}/${p.lon}`}>
                    {coords(p.lat, p.lon)}
                  </a>
                  <span className="text-gray-500">±{p.accuracy} m</span>
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Set up the beacon</h2>
        <Setup device={device} agentToken={agentToken} />
        {(device.platform === 'linux' || device.platform === 'windows') && (
          <button onClick={newToken} className="text-sm text-gray-600 hover:text-gray-900">
            Generate new agent token
          </button>
        )}
      </section>

      <section className="border-t border-gray-200 pt-4">
        <button onClick={remove} className="text-sm font-medium text-red-600 hover:text-red-800">
          Delete device
        </button>
      </section>
    </div>
  )
}
