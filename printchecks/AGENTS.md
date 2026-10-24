# PrintChecks contributor guide

This directory contains the Tauri desktop application. These instructions apply to every file under `printchecks/`.

## Scope and design

- Keep financial data local. Do not add analytics, telemetry, remote APIs, cloud synchronization, remote fonts, or network-capable dependencies without an explicit product decision and documentation update.
- The native vault lives in `Documents\PrintChecks\printchecks.vault`. Do not log bank data, passphrases, recovery keys, decrypted records, or vault contents.
- Preserve the Tauri CSP and capability boundary. Add only the smallest native permission or command required by a feature.
- Treat recovery keys as secrets. They are displayed once during vault creation or legacy-vault upgrade; never persist them in frontend storage.

## Vault work

- Keep passphrase validation and user-facing errors clear. A too-short passphrase must not be described as an unlock failure.
- Maintain atomic vault writes and the interrupted-write recovery path in `src-tauri/src/main.rs`.
- Preserve compatibility with existing V1 vaults when changing the vault format.
- Add Rust tests for cryptographic-format, recovery, or persistence changes; add frontend tests for UI and storage-boundary behavior.

## Printing and profiles

- `CheckPrinter.vue` uses a 1,200px design canvas printed at 68% zoom to yield an 8.5 × 3.5 inch check. Do not replace this with a CSS transform without verifying PDF and print pagination.
- A profile calibration must apply to both the preview CSS and the dynamically injected print CSS.
- Keep payee baselines synchronized through `src/utils/paperProfiles.ts` and its test.
- Never use a fake security-stock background for a profile that is meant to be plain. The real security background comes from check stock and is omitted at print time.
- Test calibrated changes on plain paper before using check stock. Document each measured offset and which profile it applies to.

## Commands

Run from this directory:

```powershell
npm run type-check
npm test -- --reporter=verbose --maxWorkers=1 --minWorkers=1
npm run build-only
Set-Location src-tauri; cargo test
```

Use `npm run tauri:dev` for the full desktop app. `npm run dev` does not run the native vault.

## Change hygiene

- Keep changes focused. Do not reformat unrelated files.
- Do not use `localStorage.clear()` in application features; remove only the intended keys.
- When editing user-visible security or print behavior, update `README.md` in this directory.
- Do not commit vault files, recovery keys, exported data, printer output, or any personally identifiable test data.
