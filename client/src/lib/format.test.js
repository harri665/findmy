import { describe, expect, it, vi } from 'vitest'
import { battery, duration, timeAgo } from './format.js'

describe('format', () => {
  it('timeAgo', () => {
    vi.setSystemTime(new Date(1_000_000 * 1000))
    expect(timeAgo(0)).toBe('never')
    expect(timeAgo(1_000_000 - 30)).toBe('just now')
    expect(timeAgo(1_000_000 - 600)).toBe('10 min ago')
    expect(timeAgo(1_000_000 - 3 * 3600)).toBe('3 h ago')
    expect(timeAgo(1_000_000 - 5 * 86400)).toBe('5 days ago')
    vi.useRealTimers()
  })

  it('battery reads the top two status bits', () => {
    expect([0x00, 0x40, 0x80, 0xc0].map(battery)).toEqual(['Full', 'Medium', 'Low', 'Critical'])
  })

  it('duration', () => {
    expect(duration(900)).toBe('15 min')
    expect(duration(7200)).toBe('2 h')
    expect(duration(3 * 86400)).toBe('3 days')
  })
})
