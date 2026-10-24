/**
 * Storage service for application data.
 *
 * The desktop app stores sensitive records in the native encrypted vault. The
 * browser implementation remains only for the web build and legacy migration.
 */

import { invoke } from '@tauri-apps/api/core'
import { decrypt, encrypt, isEncrypted } from './encryption'

export const SENSITIVE_KEYS = [
  'checkList',
  'printchecks_receipts',
  'printchecks_payments',
  'vendors',
  'bankAccounts',
  'printchecks_templates',
  'printchecks_receipt_templates',
  'printchecks_customization',
  'printchecks_presets',
  'printchecks_settings'
] as const

const METADATA_KEYS = ['encryption_enabled', 'encryption_test', 'encryption_migration_complete']

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function serialized(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value)
}

interface VaultOpenResult {
  recovery_key: string | null
}

class SecureStorage {
  private password: string | null = null
  private encryptionEnabled = false
  private nativeVaultUnlocked = false

  usesNativeVault(): boolean {
    return isTauriRuntime()
  }

  isNativeVaultUnlocked(): boolean {
    return this.usesNativeVault() && this.nativeVaultUnlocked
  }

  async nativeVaultExists(): Promise<boolean> {
    if (!this.usesNativeVault()) return false
    return invoke<boolean>('vault_exists')
  }

  async createNativeVault(passphrase: string, confirmation: string): Promise<string> {
    if (!this.usesNativeVault()) throw new Error('The native vault is only available in the desktop app')
    // Validate legacy records before creating a vault. Otherwise a wrong old
    // browser password could create a vault but prevent its recovery key from
    // ever reaching the setup screen.
    const legacyRecords = await this.readLegacyRecords(passphrase)
    const recoveryKey = await invoke<string>('create_vault', {
      passphrase,
      confirmation,
      initialData: Object.fromEntries(legacyRecords)
    })
    this.nativeVaultUnlocked = true
    this.removeLegacyRecords(legacyRecords)
    return recoveryKey
  }

  async unlockNativeVault(passphrase: string): Promise<string | null> {
    if (!this.usesNativeVault()) throw new Error('The native vault is only available in the desktop app')
    const result = await invoke<VaultOpenResult>('unlock_vault', { passphrase })
    this.nativeVaultUnlocked = true
    return result.recovery_key
  }

  async resetNativeVaultPassphrase(recoveryKey: string, passphrase: string, confirmation: string): Promise<void> {
    if (!this.usesNativeVault()) throw new Error('The native vault is only available in the desktop app')
    await invoke('reset_passphrase_with_recovery', { recoveryKey, passphrase, confirmation })
    this.nativeVaultUnlocked = true
  }

  async lockNativeVault(): Promise<void> {
    if (!this.usesNativeVault()) return
    this.nativeVaultUnlocked = false
    await invoke('vault_lock')
  }
  async startNewNativeVault(passphrase: string, confirmation: string): Promise<string> {
    if (!this.usesNativeVault()) throw new Error('The native vault is only available in the desktop app')
    const recoveryKey = await invoke<string>('start_new_vault', { passphrase, confirmation })
    this.nativeVaultUnlocked = true
    for (const key of SENSITIVE_KEYS) localStorage.removeItem(key)
    for (const key of METADATA_KEYS) localStorage.removeItem(key)
    return recoveryKey
  }

  private assertNativeVaultUnlocked(): void {
    if (!this.nativeVaultUnlocked) throw new Error('Unlock the vault before accessing application data')
  }

  /**
   * Moves legacy browser records into the native vault. Values are decoded
   * before browser data is removed, so a wrong password leaves them intact.
   */
  private async readLegacyRecords(passphrase: string): Promise<Array<[string, string]>> {
    const records: Array<[string, string]> = []

    for (const key of SENSITIVE_KEYS) {
      const rawValue = localStorage.getItem(key)
      if (!rawValue) continue
      const value = isEncrypted(rawValue) ? serialized(await decrypt(rawValue, passphrase)) : rawValue
      records.push([key, value])
    }

    return records
  }

  private removeLegacyRecords(records: Array<[string, string]>): void {
    for (const [key] of records) localStorage.removeItem(key)
    for (const key of METADATA_KEYS) localStorage.removeItem(key)
    sessionStorage.removeItem('encryption_password')
  }

  // Browser-only legacy encryption support.
  initialize(password: string | null = null) {
    this.password = password
    this.encryptionEnabled = localStorage.getItem('encryption_enabled') === 'true'
  }

  updatePassword(newPassword: string) {
    this.password = newPassword
  }

  private shouldEncrypt(key: string): boolean {
    return (SENSITIVE_KEYS as readonly string[]).includes(key) && !METADATA_KEYS.includes(key)
  }

  async get(key: string): Promise<string | null> {
    if (this.usesNativeVault() && this.shouldEncrypt(key)) {
      this.assertNativeVaultUnlocked()
      return invoke<string | null>('vault_get', { key })
    }

    const rawValue = localStorage.getItem(key)
    if (!rawValue || METADATA_KEYS.includes(key) || !isEncrypted(rawValue)) return rawValue
    if (!this.password) throw new Error('Password required to decrypt data')
    return serialized(await decrypt(rawValue, this.password))
  }

  async set(key: string, value: unknown): Promise<void> {
    const valueToStore = serialized(value)
    if (this.usesNativeVault() && this.shouldEncrypt(key)) {
      this.assertNativeVaultUnlocked()
      await invoke('vault_set', { key, value: valueToStore })
      return
    }
    if (METADATA_KEYS.includes(key)) {
      localStorage.setItem(key, valueToStore)
    } else if (this.encryptionEnabled && this.shouldEncrypt(key) && this.password) {
      localStorage.setItem(key, await encrypt(value, this.password))
    } else {
      localStorage.setItem(key, valueToStore)
    }
  }

  remove(key: string): void {
    localStorage.removeItem(key)
  }

  clear(): void {
    localStorage.clear()
  }

  async migrateToEncrypted(): Promise<void> {
    if (!this.password) throw new Error('Password required for migration')
    for (const key of SENSITIVE_KEYS) {
      const rawValue = localStorage.getItem(key)
      if (rawValue && !isEncrypted(rawValue)) localStorage.setItem(key, await encrypt(rawValue, this.password))
    }
    localStorage.setItem('encryption_migration_complete', 'true')
  }

  async migrateToPlainText(): Promise<void> {
    if (!this.password) throw new Error('Password required for migration')
    for (const key of SENSITIVE_KEYS) {
      const rawValue = localStorage.getItem(key)
      if (rawValue && isEncrypted(rawValue)) localStorage.setItem(key, serialized(await decrypt(rawValue, this.password)))
    }
    localStorage.removeItem('encryption_migration_complete')
  }

  async reencryptWithNewPassword(oldPassword: string, newPassword: string): Promise<void> {
    for (const key of SENSITIVE_KEYS) {
      const rawValue = localStorage.getItem(key)
      if (!rawValue || !isEncrypted(rawValue)) continue
      localStorage.setItem(key, await encrypt(await decrypt(rawValue, oldPassword), newPassword))
    }
    this.password = newPassword
  }

  needsMigration(): boolean {
    if (!this.encryptionEnabled) return false
    return SENSITIVE_KEYS.some((key) => {
      const rawValue = localStorage.getItem(key)
      return Boolean(rawValue && !isEncrypted(rawValue))
    }) || localStorage.getItem('encryption_migration_complete') !== 'true'
  }

  getStats(): { total: number; encrypted: number; plainText: number } {
    let total = 0
    let encrypted = 0
    for (const key of SENSITIVE_KEYS) {
      const rawValue = localStorage.getItem(key)
      if (!rawValue) continue
      total++
      if (isEncrypted(rawValue)) encrypted++
    }
    return { total, encrypted, plainText: total - encrypted }
  }
}

export const secureStorage = new SecureStorage()
