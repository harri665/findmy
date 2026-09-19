import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { api } from '../api.js'
import DeviceMap from '../components/DeviceMap.jsx'
import { deviceColor, timeAgo } from '../lib/format.js'

const REFRESH_MS = 60_000

export default function MapPage() {
  const [devices, setDevices] = useState(null)
  const [error, setError] = useState(null)
  const navigate = useNavigate()

  useEffect(() => {
    let cancelled = false
    const load = () =>
      api
        .devices()
        .then((d) => !cancelled && (setDevices(d), setError(null)))
        .catch((e) => !cancelled && setError(e.message))
    load()
    const timer = setInterval(load, REFRESH_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [])

  if (error) return <p className="page text-red-600">Couldn't load devices: {error}</p>
  if (!devices) return <p className="page text-gray-500">Loading…</p>

  if (!devices.length) {
    return (
      <div className="page space-y-3 text-center">
        <h1 className="text-2xl font-bold">No devices yet</h1>
        <p className="text-gray-600">Add a device to generate its keys and start tracking it.</p>
        <Link to="/devices/new" className="inline-block rounded-md bg-gray-900 px-4 py-2 font-medium text-white">
          Add device
        </Link>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col md:flex-row">
      <aside className="max-h-48 overflow-y-auto border-b border-gray-200 bg-white md:max-h-none md:w-72 md:border-b-0 md:border-r">
        {devices.map((d) => (
          <Link key={d.id} to={`/devices/${d.id}`} className="flex items-center gap-3 border-b border-gray-100 px-4 py-3 hover:bg-gray-50">
            <span className="h-3 w-3 shrink-0 rounded-full" style={{ background: deviceColor(d.id) }} />
            <span className="text-xl">{d.icon}</span>
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{d.name}</span>
              <span className="block text-xs text-gray-500">{d.last ? `seen ${timeAgo(d.last.ts)}` : 'no reports yet'}</span>
            </span>
          </Link>
        ))}
      </aside>
      <div className="min-h-[60vh] flex-1 p-2 md:min-h-0">
        <DeviceMap devices={devices} onSelect={(id) => navigate(`/devices/${id}`)} />
      </div>
    </div>
  )
}
