import { useEffect, useState } from 'react'
import { NavLink, Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import './App.css'
import { api, getToken, setToken, setUnauthorizedHandler } from './api.js'
import Login from './pages/Login.jsx'
import MapPage from './pages/MapPage.jsx'
import Devices from './pages/Devices.jsx'
import NewDevice from './pages/NewDevice.jsx'
import DeviceDetail from './pages/DeviceDetail.jsx'
import Settings from './pages/Settings.jsx'
import ComingSoon from './pages/ComingSoon.jsx'

function NavItem({ to, children }) {
  return (
    <NavLink
      to={to}
      end={to === '/'}
      className={({ isActive }) =>
        `rounded-md px-2 py-1.5 text-sm font-medium sm:px-3 ${isActive ? 'bg-gray-900 text-white' : 'text-gray-700 hover:bg-gray-200'}`
      }
    >
      {children}
    </NavLink>
  )
}

function Dashboard() {
  const [authed, setAuthed] = useState(() => Boolean(getToken()))
  const navigate = useNavigate()

  useEffect(() => {
    setUnauthorizedHandler(() => setAuthed(false))
  }, [])

  async function logout() {
    try {
      await api.logout()
    } catch {
      // Logging out locally is what matters.
    }
    setToken(null)
    setAuthed(false)
    navigate('/')
  }

  if (!authed) return <Login onLogin={() => setAuthed(true)} />

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-2 border-b border-gray-200 bg-white px-3 py-2 sm:px-4">
        <span className="shrink-0 text-lg font-bold sm:mr-4">📍 findmy</span>
        <nav className="flex flex-1 gap-0.5 sm:gap-1">
          <NavItem to="/">Map</NavItem>
          <NavItem to="/devices">Devices</NavItem>
          <NavItem to="/settings">Settings</NavItem>
        </nav>
        <button onClick={logout} className="shrink-0 text-sm text-gray-600 hover:text-gray-900">
          Log out
        </button>
      </header>

      <main className="min-h-0 flex-1">
        <Routes>
          <Route path="/" element={<MapPage />} />
          <Route path="/devices" element={<Devices />} />
          <Route path="/devices/new" element={<NewDevice />} />
          <Route path="/devices/:id" element={<DeviceDetail />} />
          <Route path="/settings" element={<Settings />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}

function App() {
  return import.meta.env.VITE_COMING_SOON === 'true' ? <ComingSoon /> : <Dashboard />
}

export default App
