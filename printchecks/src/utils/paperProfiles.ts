export type PaperProfileId =
  | 'generic-blank-top'
  | 'versacheck-1000'
  | 'versacheck-1000-center'
  | 'checkomatic-top'
  | 'checkomatic-top-center'

/** The check payee must share the same baseline across paper profiles. */
export const PAYEE_TOP_BY_PROFILE: Record<PaperProfileId, string> = {
  'generic-blank-top': '178px',
  'versacheck-1000': '178px',
  'versacheck-1000-center': '178px',
  'checkomatic-top': '178px',
  'checkomatic-top-center': '178px'
}

export function payeeTopForProfile(profile: PaperProfileId): string {
  return PAYEE_TOP_BY_PROFILE[profile]
}