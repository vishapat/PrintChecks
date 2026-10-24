import { describe, expect, it } from 'vitest'
import { PAYEE_TOP_BY_PROFILE, payeeTopForProfile } from '@/utils/paperProfiles'

describe('paper profile payee alignment', () => {
  it('keeps every profile on the established payee baseline', () => {
    for (const profile of Object.keys(PAYEE_TOP_BY_PROFILE) as Array<keyof typeof PAYEE_TOP_BY_PROFILE>) {
      expect(payeeTopForProfile(profile)).toBe('178px')
    }
  })
})