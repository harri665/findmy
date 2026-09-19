export function timeAgo(unixSeconds) {
  if (!unixSeconds) return 'never'
  const s = Math.max(0, Math.floor(Date.now() / 1000 - unixSeconds))
  if (s < 60) return 'just now'
  const m = Math.floor(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.floor(m / 60)
  if (h < 48) return `${h} h ago`
  return `${Math.floor(h / 24)} days ago`
}

export function dateTime(unixSeconds) {
  return new Date(unixSeconds * 1000).toLocaleString()
}

export function coords(lat, lon) {
  return `${lat.toFixed(5)}, ${lon.toFixed(5)}`
}

const BATTERY = ['Full', 'Medium', 'Low', 'Critical']
export function battery(status) {
  return BATTERY[(status >> 6) & 3]
}

export function duration(seconds) {
  if (seconds < 3600) return `${Math.round(seconds / 60)} min`
  if (seconds < 172800) return `${(seconds / 3600).toFixed(1).replace(/\.0$/, '')} h`
  return `${(seconds / 86400).toFixed(1).replace(/\.0$/, '')} days`
}

export const PLATFORMS = [
  { id: 'esp32', label: 'ESP32 tag' },
  { id: 'nrf52', label: 'nRF52 tag' },
  { id: 'linux', label: 'Linux / Raspberry Pi' },
  { id: 'windows', label: 'Windows PC' },
  { id: 'other', label: 'Other' },
]

export const ICONS = ['🏷️', '🚲', '🎒', '🔑', '🚗', '💻', '🧳', '🐶', '📦']

// Distinct colours for device markers.
const COLORS = ['#2563eb', '#dc2626', '#16a34a', '#d97706', '#7c3aed', '#db2777', '#0891b2', '#65a30d']
export function deviceColor(id) {
  return COLORS[id % COLORS.length]
}
