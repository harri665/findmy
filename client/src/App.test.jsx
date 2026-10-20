import { afterEach, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import App from './App.jsx'

afterEach(() => vi.unstubAllEnvs())

it('shows the coming soon page by default', () => {
  vi.stubEnv('VITE_COMING_SOON', undefined)

  render(
    <MemoryRouter>
      <App />
    </MemoryRouter>,
  )

  expect(screen.getByRole('heading', { name: 'Coming soon' })).toBeInTheDocument()
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
  expect(screen.queryByText('Coming soon')).not.toBeInTheDocument()
})
