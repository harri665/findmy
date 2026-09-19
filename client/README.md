# findmy dashboard

React 19 + Vite + Tailwind 4 + React Router, based on
[harrisonwebdefault](https://github.com/harri665/harrisonwebdefault). The map
uses Leaflet with OpenStreetMap tiles.

- `npm run dev`: Vite on :5173, proxying `/api` to fm-server on :3001
  (run both together with `npm run dev` from the repo root).
- `npm run build`: static build in `dist/`. The Dockerfile serves it with nginx
  and proxies `/api/` to the `server` container.

Pages: map (`/`), devices (`/devices`), add device (`/devices/new`), device
detail with history (`/devices/:id`), settings (`/settings`).
