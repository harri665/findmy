const REPO_URL = 'https://github.com/harri665/findmy'

const STEPS = [
  {
    title: 'Build a beacon',
    body: 'Flash an ESP32-C3 or run the Linux daemon to broadcast a Find My key.',
  },
  {
    title: 'Nearby iPhones report it',
    body: 'Apple devices that pass by send its location to Apple, encrypted.',
  },
  {
    title: 'See it on your map',
    body: 'Your server fetches and decrypts the reports. Only you can read them.',
  },
]

export default function ComingSoon() {
  return (
    <div className="flex min-h-full flex-col items-center justify-center bg-white px-4 py-16 text-center">
      <div className="relative flex h-16 w-16 items-center justify-center">
        <span
          className="absolute inline-flex h-full w-full animate-ping rounded-full bg-gray-200 motion-reduce:animate-none"
          aria-hidden="true"
        />
        <span className="relative flex h-16 w-16 items-center justify-center rounded-full bg-gray-100 text-3xl">
          📍
        </span>
      </div>

      <p className="mt-6 text-lg font-semibold text-gray-900">findmy</p>
      <h1 className="mt-2 text-4xl font-bold tracking-tight text-gray-900 sm:text-5xl">Coming soon</h1>
      <p className="mt-4 max-w-md text-base leading-relaxed text-gray-600">
        Self-hosted tracking for your own Bluetooth beacons on Apple&apos;s Find My network.
      </p>

      <ol className="mt-12 grid w-full max-w-3xl gap-4 text-left sm:grid-cols-3">
        {STEPS.map((step, i) => (
          <li key={step.title} className="rounded-xl border border-gray-200 p-5">
            <span className="flex h-7 w-7 items-center justify-center rounded-full bg-gray-900 text-xs font-semibold text-white">
              {i + 1}
            </span>
            <h2 className="mt-3 text-sm font-semibold text-gray-900">{step.title}</h2>
            <p className="mt-1 text-sm leading-relaxed text-gray-600">{step.body}</p>
          </li>
        ))}
      </ol>

      <a
        href={REPO_URL}
        className="mt-10 rounded-md bg-gray-900 px-5 py-2.5 text-sm font-medium text-white hover:bg-gray-700 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gray-900"
      >
        View on GitHub
      </a>
      <p className="mt-6 text-xs text-gray-500">Only track things you own.</p>
    </div>
  )
}
