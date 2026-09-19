import { Fragment, useEffect } from 'react'
import { MapContainer, TileLayer, Circle, CircleMarker, Polyline, Popup, Tooltip, useMap } from 'react-leaflet'
import { deviceColor, timeAgo, coords } from '../lib/format.js'

// Fit the view to the given points whenever they change.
function FitBounds({ points }) {
  const map = useMap()
  const key = JSON.stringify(points)
  useEffect(() => {
    const pts = JSON.parse(key)
    if (pts.length === 1) map.setView(pts[0], 16)
    else if (pts.length > 1) map.fitBounds(pts, { padding: [40, 40], maxZoom: 17 })
  }, [map, key])
  return null
}

/**
 * devices: [{ id, name, icon, last: { lat, lon, accuracy, ts } | null }]
 * history: optional [{ lat, lon, ts, accuracy }], drawn as a path
 */
export default function DeviceMap({ devices = [], history = [], onSelect, className = '' }) {
  const located = devices.filter((d) => d.last)
  const points = history.length ? history.map((p) => [p.lat, p.lon]) : located.map((d) => [d.last.lat, d.last.lon])
  const pathColor = devices.length === 1 ? deviceColor(devices[0].id) : '#2563eb'

  return (
    <MapContainer center={[20, 0]} zoom={2} className={`h-full w-full rounded-lg ${className}`} scrollWheelZoom>
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
      />
      <FitBounds points={points} />

      {history.length > 1 && (
        <Polyline positions={history.map((p) => [p.lat, p.lon])} pathOptions={{ color: pathColor, weight: 3, opacity: 0.6 }} />
      )}
      {history.map((p, i) => (
        <CircleMarker
          key={`${p.ts}-${i}`}
          center={[p.lat, p.lon]}
          radius={i === history.length - 1 ? 7 : 4}
          pathOptions={{ color: pathColor, fillOpacity: i === history.length - 1 ? 0.9 : 0.4 }}
        >
          <Tooltip>
            {timeAgo(p.ts)} · ±{p.accuracy} m
          </Tooltip>
        </CircleMarker>
      ))}

      {!history.length &&
        located.map((d) => (
          <Fragment key={d.id}>
            <Circle
              center={[d.last.lat, d.last.lon]}
              radius={Math.max(d.last.accuracy, 5)}
              pathOptions={{ color: deviceColor(d.id), weight: 1, fillOpacity: 0.1 }}
            />
            <CircleMarker
              center={[d.last.lat, d.last.lon]}
              radius={9}
              pathOptions={{ color: 'white', weight: 2, fillColor: deviceColor(d.id), fillOpacity: 1 }}
              eventHandlers={{ click: () => onSelect?.(d.id) }}
            >
              <Tooltip direction="top" offset={[0, -8]} permanent>
                {d.icon} {d.name}
              </Tooltip>
              <Popup>
                <strong>{d.name}</strong>
                <br />
                {coords(d.last.lat, d.last.lon)} · ±{d.last.accuracy} m
                <br />
                seen {timeAgo(d.last.ts)}
              </Popup>
            </CircleMarker>
          </Fragment>
        ))}
    </MapContainer>
  )
}
