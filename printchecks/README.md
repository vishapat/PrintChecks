# PrintChecks

PrintChecks is a local desktop application for preparing and printing occasional business or personal checks. It is built with Vue, TypeScript, and Tauri. The desktop vault stores bank accounts, vendors, settings, and payment history in an encrypted file on the computer.

## Local-only operation

The packaged application has no application backend, telemetry, analytics, or remote runtime API. Tauri's content-security policy allows only local application assets and its local IPC bridge. The application stores its vault at:

`Documents\PrintChecks\printchecks.vault`

The vault file is encrypted with XChaCha20-Poly1305. The passphrase is processed with Argon2 before it wraps the vault encryption key. Creating a vault also creates a recovery key. Save that key outside the computer; it is the only way to reset a forgotten passphrase without discarding the vault.

The build process and dependency installation contact package registries. Treat that as development activity, separate from normal app use.

## Run it

Requirements: Node.js 18 or later, Rust, and the Windows WebView2 runtime.

```powershell
cd C:\Users\visha\git\PrintChecks\printchecks
npm install
npm run tauri:dev
```

`npm run dev` starts only the browser development server. Use `npm run tauri:dev` to exercise the desktop vault and native application behavior.

## Vault lifecycle

1. Start the app and create a vault with a passphrase of at least 12 characters.
2. Save the generated recovery key when prompted.
3. Unlock with the passphrase each time the desktop app opens.
4. Use the recovery key only to set a new passphrase.
5. If both are lost, **Start a new vault** archives the encrypted file and begins with no data.

Exports are encrypted and must be unlocked with their own export passphrase when imported. The import and export screens let you select bank accounts, vendors, history, and settings independently.

## Check paper profiles

Each profile targets a letter-size sheet with a 3.5 × 8.5 inch top check. Always run a test on plain paper before using check stock.

| Profile | Intended stock/layout | MICR calibration |
| --- | --- | --- |
| Generic blank business check | Plain top voucher stock | Base position |
| VersaCheck Form #1000 | Blue security stock | VersaCheck position |
| VersaCheck 2 | Plain, centered bank identity with return and recipient windows | VersaCheck position |
| CheckOMatic | Top business voucher | 2.5 mm lower than VersaCheck #1000 |
| CheckOMatic 2 | Plain centered bank identity with return and recipient windows | 2.5 mm lower than VersaCheck #1000 |

The profile selection is saved locally. Bank-specific MICR ordering remains a bank-account setting; verify the routing, account, and check-number arrangement with the financial institution before relying on a printed check.

## Development and verification

```powershell
npm run type-check
npm test -- --reporter=verbose --maxWorkers=1 --minWorkers=1
npm run build-only
Set-Location src-tauri
cargo test
```

Run all four checks after changes that touch vault behavior, printing, paper profiles, imports, or the Tauri boundary. Use `npm run build` as the normal production frontend build. Use `npm run tauri:dev` for manual print-preview and vault testing.

## Project map

- `src/components/CheckPrinter.vue` — check preview, print stylesheet, paper profiles, and calibration.
- `src/services/secureStorage.ts` — frontend vault boundary and legacy local-storage migration.
- `src-tauri/src/main.rs` — encrypted vault, recovery key, and native commands.
- `src/utils/paperProfiles.ts` — profile IDs and payee alignment baseline.
- `src/__tests__/` — frontend unit tests.

## Print safety

Paper feeds, printer scaling, ink, and check-stock geometry vary. Select actual size / 100% scaling in the printer dialog, print a plain-paper test, and compare it against the stock before printing a negotiable check. The user remains responsible for confirming that their bank accepts the check and MICR line.
