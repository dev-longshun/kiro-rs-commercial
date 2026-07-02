const MICROSOFT_ISSUER_PATTERN = /https:\/\/login\.microsoftonline\.com\/[^/\s"'<>]+\/v2\.0/i

function trimNonEmpty(value?: string | null): string | undefined {
  const trimmed = value?.trim()
  return trimmed ? trimmed : undefined
}

export function extractMicrosoftIssuerUrl(...values: Array<string | null | undefined>): string | undefined {
  for (const value of values) {
    const trimmed = trimNonEmpty(value)
    if (!trimmed) continue

    const match = trimmed.match(MICROSOFT_ISSUER_PATTERN)
    if (match) {
      return match[0].replace(/\/+$/, '')
    }
  }

  return undefined
}

export function deriveMicrosoftTokenEndpoint(issuerUrl?: string | null): string | undefined {
  const issuer = extractMicrosoftIssuerUrl(issuerUrl)
  if (!issuer) return undefined

  return issuer.replace(/\/v2\.0$/i, '/oauth2/v2.0/token')
}

export function defaultExternalIdpScopes(clientId?: string | null): string | undefined {
  const id = trimNonEmpty(clientId)
  if (!id) return undefined

  return [
    `api://${id}/codewhisperer:conversations`,
    `api://${id}/codewhisperer:completions`,
    'offline_access',
  ].join(' ')
}

interface ExternalIdpMetadataInput {
  clientId?: string
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
  userId?: string | null
}

interface ExternalIdpMetadata {
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
}

export function resolveExternalIdpMetadata(input: ExternalIdpMetadataInput): ExternalIdpMetadata {
  const issuerUrl = extractMicrosoftIssuerUrl(input.issuerUrl, input.userId)
  const tokenEndpoint = trimNonEmpty(input.tokenEndpoint) || deriveMicrosoftTokenEndpoint(issuerUrl)
  const scopes = trimNonEmpty(input.scopes) || defaultExternalIdpScopes(input.clientId)

  return {
    tokenEndpoint,
    issuerUrl,
    scopes,
  }
}

export function normalizeExpiresAt(value: unknown): string | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    const millis = value > 1_000_000_000_000 ? value : value * 1000
    return new Date(millis).toISOString()
  }

  if (typeof value !== 'string') return undefined

  const trimmed = value.trim()
  if (!trimmed) return undefined

  const numeric = Number(trimmed)
  if (Number.isFinite(numeric)) {
    const millis = numeric > 1_000_000_000_000 ? numeric : numeric * 1000
    return new Date(millis).toISOString()
  }

  const parsed = Date.parse(trimmed)
  if (Number.isNaN(parsed)) return undefined

  return new Date(parsed).toISOString()
}
