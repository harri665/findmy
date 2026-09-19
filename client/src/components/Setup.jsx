import { useState } from 'react'
import { api } from '../api.js'

function Code({ children }) {
  return <pre className="overflow-x-auto rounded-md bg-gray-900 p-3 text-xs text-gray-100">{children}</pre>
}

/** How to put a device's keys onto its beacon, per platform. */
export default function Setup({ device, agentToken }) {
  const [error, setError] = useState(null)
  const file = `${device.name.replace(/[^\w-]+/g, '-').toLowerCase() || 'device'}.fmkt`
  const origin = window.location.origin

  async function download() {
    setError(null)
    try {
      await api.downloadBlob(device.id, file)
    } catch (e) {
      setError(e.message)
    }
  }

  const blobButton = (
    <button onClick={download} className="rounded-md bg-gray-900 px-4 py-2 text-sm font-medium text-white hover:bg-gray-700">
      Download key blob ({file})
    </button>
  )

  return (
    <div className="space-y-3 text-sm">
      {(device.platform === 'esp32' || device.platform === 'other') && (
        <>
          <p>Flash the findmy firmware once, then write this device's keys to the board's key partition:</p>
          {blobButton}
          <Code>{`espflash write-bin --chip esp32c3 0x210000 ${file}`}</Code>
          <p className="text-gray-600">See beacons/esp32/README.md for building and flashing the firmware.</p>
        </>
      )}
      {device.platform === 'nrf52' && (
        <>
          <p>nRF52 firmware isn't built yet. Download the key blob now and keep it for later.</p>
          {blobButton}
        </>
      )}
      {(device.platform === 'linux' || device.platform === 'windows') && (
        <>
          <p>
            The {device.platform === 'linux' ? 'Linux daemon' : 'Windows agent'} downloads its keys from this server using the
            device's agent token.
          </p>
          {agentToken ? (
            <>
              <p className="font-medium text-amber-700">Copy the agent token now. It won't be shown again.</p>
              <Code>{`FM_SERVER=${origin}\nFM_AGENT_TOKEN=${agentToken}`}</Code>
            </>
          ) : (
            <p className="text-gray-600">Lost the token? Generate a new one on the device page (the old one stops working).</p>
          )}
          <p className="text-gray-600">
            The {device.platform === 'linux' ? 'Linux daemon' : 'Windows agent'} isn't built yet; you can also download the blob
            directly.
          </p>
          {blobButton}
        </>
      )}
      {error && <p className="text-red-600">{error}</p>}
    </div>
  )
}
