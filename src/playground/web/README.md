# playground · web

Vite + Svelte 5 + TypeScript frontend for the diff-fusion playground.

## Build

```bash
cd playground/web
npm install
npm run build
```

`npm run build` outputs to `dist/`, which the Rust server (`playground/src/main.rs`) serves via `tower_http::services::ServeDir`. **You must run `npm run build` before launching `cargo run -p playground`** — the Rust server has no dev-mode hot reload of its own.

## Develop with hot reload

```bash
# terminal A
cargo run -p playground            # backend on :3000

# terminal B
cd playground/web
npm run dev                        # Vite dev server with HMR on :5173
```

Vite proxies `/sync`, `/api/*` to the Rust server (see `vite.config.ts`). Open http://localhost:5173 in dev; http://localhost:3000 for the built version.

## Layout

- `src/App.svelte` — root, composes the watch panel + demo form + dialog.
- `src/components/`
  - `WatchPanel.svelte` — runs list, cycles list, EventSource.
  - `DemoForm.svelte` — JSON inputs + Run Sync.
  - `RunDialog.svelte` — modal that wraps Pipeline + OutcomeDetail.
  - `Pipeline.svelte` / `OutcomeDetail.svelte` / `FieldChangelog.svelte` — display.
- `src/lib/`
  - `types.ts` — wire types matching `playground/src/dto.rs` and `src/ports/observer.rs`.
  - `api.ts`, `observe.ts`, `samples.ts`, `render.ts`, `runState.svelte.ts`.
- `src/styles/legacy.css` — visual styling carried over from the previous vanilla build.
