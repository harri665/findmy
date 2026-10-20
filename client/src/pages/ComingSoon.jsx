export default function ComingSoon() {
  return (
    <div className="flex h-full items-center justify-center p-4">
      <div className="w-full max-w-md space-y-4 rounded-xl border border-gray-200 bg-white p-8 text-center shadow-sm">
        <div className="inline-flex items-center gap-2 text-2xl font-bold text-gray-900">
          <span aria-hidden="true">📍</span>
          <span>findmy</span>
        </div>
        <div className="pt-1">
          <span className="inline-block rounded-full bg-gray-900 px-3 py-1 text-xs font-medium tracking-wide text-white uppercase">
            Coming Soon
          </span>
        </div>
        <h1 className="text-xl font-semibold text-gray-900">
          Self-hosted Find My beacon tracking
        </h1>
        <p className="text-sm leading-relaxed text-gray-600">
          Build your own Bluetooth LE beacons and track them on Apple&apos;s Find My
          network with a self-hosted map and location history dashboard.
        </p>
      </div>
    </div>
  )
}
