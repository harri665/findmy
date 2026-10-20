import { afterEach, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import App from './App.jsx'

afterEach(() => vi.unstubAllEnvs())

it('shows the coming soon page instead of the dashboard in Docker mode', () => {
  vi.stubEnv('VITE_COMING_SOON', 'true')

  render(
    <MemoryRouter initialEntries={['/devices']}>
      <App />
    </MemoryRouter>,
  )

  expect(screen.getByText('Coming Soon')).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: 'Self-hosted Find My beacon tracking' })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Sign in' })).not.toBeInTheDocument()
  expect(screen.queryByRole('navigation')).not.toBeInTheDocument()
})

it('shows the dashboard login when coming soon mode is disabled', () => {
  vi.stubEnv('VITE_COMING_SOON', 'false')
  localStorage.clear()

  render(
    <MemoryRouter>
      <App />
    </MemoryRouter>,
  )

  expect(screen.getByLabelText('Password')).toBeInTheDocument()
  expect(screen.queryByText('Coming Soon')).not.toBeInTheDocument()
})
