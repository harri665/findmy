# Fetch location reports

The server fetches encrypted reports from Apple every 15 minutes (configurable
from 5 to 1440), decrypts them locally, and updates device history. It requests
the last seven days each time, so restarts and temporary outages do not create
gaps within that window. Previously stored reports are deduplicated.

## Connect an Apple account

1. Run an anisette provider with a JSON header endpoint returning
   `X-Apple-I-MD` and `X-Apple-I-MD-M`, optionally `X-Apple-I-MD-RINFO`.
   A provider is an external dependency; this repo includes its HTTP client.
   Use the same provider for login and report retrieval. It must be reachable
   from both your login machine and the server. `localhost` in Docker refers
   to the container; use a service name on a shared network or a reachable host.
2. With Python 3.10–3.14, install the login helper's dependencies and sign in:

   ```sh
   python -m venv .venv
   # Linux/macOS: source .venv/bin/activate
   # Windows PowerShell: .\.venv\Scripts\Activate.ps1
   python -m pip install -r tools/apple/requirements.txt
   python tools/apple/login.py --anisette-url http://your-provider:6969
   ```

   The helper prompts for your password without echoing it, supports trusted
   device and SMS verification, and writes `findmy.apple-session.json`. It
   refuses to overwrite an existing file; use `--output new.apple-session.json`
   when renewing your session. It exports the report token and persistent
   identity fields, without saving your Apple password.
3. Open **Settings** in the dashboard, enter the provider's URL as seen by the
   server, and save. Import the session JSON, add a device if needed, and click
   **Fetch now**. Scheduled fetching starts once the provider and session are
   configured. Settings shows errors, report counts, and the last and next
   fetch times.

Session files are secrets. They are ignored by Git and Docker builds, and the
server encrypts the imported token with its master key. Back up that key with
the database. Use HTTPS when accessing the dashboard remotely.

An existing FindMy.py account JSON export can also be imported. The server
keeps only `dsid`, `searchPartyToken`, `uid`, and `device_id`, discarding account
passwords and unrelated tokens. If Apple rejects or expires the session, run
the helper again and import the new file. **Remove session** disables automatic
retrieval without deleting device keys or history.

## API

All of these routes require the dashboard's `Authorization: Bearer <token>`:

| Method | Route | Behavior |
|---|---|---|
| GET | `/api/apple` | Connection and fetch status; never returns credentials |
| PUT | `/api/apple` | Import a session JSON document |
| DELETE | `/api/apple` | Remove the encrypted session |
| POST | `/api/reports/fetch` | Fetch reports now; 409 if a fetch is in progress, 400 if unconfigured, 502 on provider/Apple failure |
| GET/PUT | `/api/settings` | Provider URL and fetch interval |
| GET | `/api/keys/hashed` | Report IDs for an optional external fetcher |
| POST | `/api/reports` | Ingest `{ "reports": [{ "hashed": "base64 SHA256", "payload": "base64 report" }] }` |

Fetches use batches of up to 256 keys. Each successful batch is saved before
the next begins; a later failure preserves earlier batches. The next scheduled
attempt retries the full seven-day window. Empty Apple responses receive two
bounded retries. Network requests time out and TLS certificates are verified.

## Verification limits

Tests exercise the HTTP exchange with a local fixture and independently
generated encrypted reports. Live retrieval requires your account, provider,
and reports uploaded by nearby Apple devices. BLE firmware still needs testing
on the intended hardware; Windows and nRF52 beacon implementations are future
extensions.

The retrieval protocol and login-helper API follow the primary
[FindMy.py implementation](https://github.com/malmeloo/FindMy.py), using its
current `findmyservice/v2/fetch` request and response formats.
