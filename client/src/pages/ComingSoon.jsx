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

      <h1 className="mt-6 text-4xl font-bold tracking-tight text-gray-900 sm:text-5xl">Coming soon</h1>
    </div>
  )
}
