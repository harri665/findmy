import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'

vi.mock('../api.js', () => ({
  api: { devices: vi.fn(), login: vi.fn(), createDevice: vi.fn() },
  setToken: vi.fn(),
  ApiError: class extends Error {},
}))
// Leaflet needs a real layout engine; the map is covered by the build, not jsdom.
vi.mock('../components/DeviceMap.jsx', () => ({
  default: ({ devices }) => <div data-testid="map">{devices.length} on map</div>,
}))

import { api, setToken } from '../api.js'
import Devices from './Devices.jsx'
import Login from './Login.jsx'
import MapPage from './MapPage.jsx'
import NewDevice from './NewDevice.jsx'

const device = (over = {}) => ({
  id: 1,
  name: 'Bike',
  icon: '🚲',
  platform: 'linux',
  key_mode: 'pool',
  key_count: 96,
  report_count: 3,
  last: { ts: Math.floor(Date.now() / 1000) - 120, lat: 40, lon: -105, accuracy: 30, confidence: 3, status: 0x40 },
  ...over,
})

const inRouter = (ui) => render(<MemoryRouter>{ui}</MemoryRouter>)

beforeEach(() => vi.clearAllMocks())

describe('Login', () => {
  it('stores the token and calls onLogin', async () => {
    api.login.mockResolvedValue({ token: 'abc' })
    const onLogin = vi.fn()
    render(<Login onLogin={onLogin} />)
    await userEvent.type(screen.getByLabelText(/password/i), 'hunter2hunter2')
    await userEvent.click(screen.getByRole('button'))
    await waitFor(() => expect(onLogin).toHaveBeenCalled())
    expect(api.login).toHaveBeenCalledWith('hunter2hunter2')
    expect(setToken).toHaveBeenCalledWith('abc')
  })

  it('shows a wrong-password message on 401', async () => {
    api.login.mockRejectedValue(Object.assign(new Error('nope'), { status: 401 }))
    render(<Login onLogin={vi.fn()} />)
    await userEvent.type(screen.getByLabelText(/password/i), 'wrongwrong')
    await userEvent.click(screen.getByRole('button'))
    expect(await screen.findByText('Wrong password')).toBeInTheDocument()
  })
})

describe('Devices', () => {
  it('lists devices with key mode and last seen', async () => {
    api.devices.mockResolvedValue([device()])
    inRouter(<Devices />)
    expect(await screen.findByText(/Bike/)).toBeInTheDocument()
    expect(screen.getByText('96 · pool')).toBeInTheDocument()
    expect(screen.getByText('2 min ago')).toBeInTheDocument()
  })

  it('shows an empty state and errors', async () => {
    api.devices.mockResolvedValue([])
    inRouter(<Devices />)
    expect(await screen.findByText('No devices yet.')).toBeInTheDocument()
  })

  it('shows the error message when loading fails', async () => {
    api.devices.mockRejectedValue(new Error('server down'))
    inRouter(<Devices />)
    expect(await screen.findByText('server down')).toBeInTheDocument()
  })
})

describe('MapPage', () => {
  it('passes the loaded devices to the map', async () => {
    api.devices.mockResolvedValue([device(), device({ id: 2, name: 'Bag' })])
    inRouter(<MapPage />)
    expect(await screen.findByTestId('map')).toHaveTextContent('2 on map')
  })

  it('reports a load failure', async () => {
    api.devices.mockRejectedValue(new Error('boom'))
    inRouter(<MapPage />)
    expect(await screen.findByText(/Couldn't load devices: boom/)).toBeInTheDocument()
  })
})

describe('NewDevice', () => {
  it('only offers derived keys off ESP32 tags', async () => {
    inRouter(<NewDevice />)
    const derived = screen.getByRole('radio', { name: /Derived from one seed/ })
    expect(derived).toBeDisabled() // ESP32 is the default platform
    await userEvent.selectOptions(screen.getByRole('combobox'), 'linux')
    expect(derived).toBeEnabled()
  })
})
