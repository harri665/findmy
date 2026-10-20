const REPO_URL = 'https://github.com/harri665/findmy'

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
      <a
        href={REPO_URL}
        className="mt-8 rounded-md bg-gray-900 px-5 py-2.5 text-sm font-medium text-white hover:bg-gray-700 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gray-900"
      >
        View on GitHub
      </a>
    </div>
  )
}
