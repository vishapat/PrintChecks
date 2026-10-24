import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/services/encryption', () => ({
  decrypt: vi.fn(),
  encrypt: vi.fn(),
  isEncrypted: vi.fn(() => false),
}))

import { secureStorage } from '@/services/secureStorage'

describe('native vault migration', () => {
  const localValues = new Map<string, string>()
  const sessionValues = new Map<string, string>()
  const localStorageMock = {
    getItem: (key: string) => localValues.get(key) ?? null,
    setItem: (key: string, value: string) => localValues.set(key, value),
    removeItem: (key: string) => localValues.delete(key),
    clear: () => localValues.clear(),
  }
  const sessionStorageMock = {
    getItem: (key: string) => sessionValues.get(key) ?? null,
    setItem: (key: string, value: string) => sessionValues.set(key, value),
    removeItem: (key: string) => sessionValues.delete(key),
    clear: () => sessionValues.clear(),
  }

  beforeEach(() => {
    vi.clearAllMocks()
    localValues.clear()
    sessionValues.clear()
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
    vi.stubGlobal('localStorage', localStorageMock)
    vi.stubGlobal('sessionStorage', sessionStorageMock)
  })

  afterEach(() => vi.unstubAllGlobals())

  it('moves legacy records into the vault before removing their browser copies', async () => {
    localStorage.setItem('bankAccounts', '[{"name":"Operating"}]')
    localStorage.setItem('encryption_enabled', 'true')
    sessionStorage.setItem('encryption_password', 'old password')
    invoke.mockResolvedValueOnce('PC-RK-example')

    await secureStorage.createNativeVault('a long enough passphrase', 'a long enough passphrase')

    expect(invoke).toHaveBeenCalledWith('create_vault', {
      passphrase: 'a long enough passphrase',
      confirmation: 'a long enough passphrase',
      initialData: { bankAccounts: '[{"name":"Operating"}]' },
    })
    expect(localStorage.getItem('bankAccounts')).toBeNull()
    expect(localStorage.getItem('encryption_enabled')).toBeNull()
    expect(sessionStorage.getItem('encryption_password')).toBeNull()
  })

  it('keeps legacy records when native vault creation fails', async () => {
    localStorage.setItem('vendors', '[{"name":"Vendor"}]')
    invoke.mockRejectedValueOnce(new Error('Vault write failed'))

    await expect(
      secureStorage.createNativeVault('a long enough passphrase', 'a long enough passphrase'),
    ).rejects.toThrow('Vault write failed')

    expect(localStorage.getItem('vendors')).toBe('[{"name":"Vendor"}]')
  })
})
