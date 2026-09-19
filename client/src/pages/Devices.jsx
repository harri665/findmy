import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { api } from '../api.js'
import { PLATFORMS, battery, timeAgo } from '../lib/format.js'

export default function Devices() {
  const [devices, setDevices] = useState(null)
  const [error, setError] = useState(null)

  useEffect(() => {
    api.devices().then(setDevices).catch((e) => setError(e.message))
  }, [])

  return (
    <div className="page space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Devices</h1>
        <Link to="/devices/new" className="rounded-md bg-gray-900 px-4 py-2 text-sm font-medium text-white hover:bg-gray-700">
          Add device
        </Link>
      </div>

      {error && <p className="text-red-600">{error}</p>}
      {!devices && !error && <p className="text-gray-500">Loading…</p>}
      {devices?.length === 0 && <p className="text-gray-600">No devices yet.</p>}

      {devices?.length > 0 && (
        <div className="overflow-x-auto rounded-lg border border-gray-200 bg-white">
          <table className="w-full text-left text-sm">
            <thead className="bg-gray-50 text-gray-600">
              <tr>
                <th className="px-4 py-2 font-medium">Device</th>
                <th className="px-4 py-2 font-medium">Type</th>
                <th className="px-4 py-2 font-medium">Keys</th>
                <th className="px-4 py-2 font-medium">Last seen</th>
                <th className="px-4 py-2 font-medium">Reports</th>
                <th className="px-4 py-2 font-medium">Battery</th>
              </tr>
            </thead>
            <tbody>
              {devices.map((d) => (
                <tr key={d.id} className="border-t border-gray-100 hover:bg-gray-50">
                  <td className="px-4 py-2">
                    <Link to={`/devices/${d.id}`} className="font-medium hover:underline">
                      {d.icon} {d.name}
                    </Link>
                  </td>
                  <td className="px-4 py-2">{PLATFORMS.find((p) => p.id === d.platform)?.label ?? d.platform}</td>
                  <td className="px-4 py-2">
                    {d.key_count} · {d.key_mode}
                  </td>
                  <td className="px-4 py-2">{d.last ? timeAgo(d.last.ts) : '—'}</td>
                  <td className="px-4 py-2">{d.report_count}</td>
                  <td className="px-4 py-2">{d.last ? battery(d.last.status) : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
