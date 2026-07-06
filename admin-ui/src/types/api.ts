// 凭据状态响应
export interface CredentialsStatusResponse {
  total: number
  available: number
  currentId: number
  credentials: CredentialStatusItem[]
}

// 单个凭据状态
export interface CredentialStatusItem {
  id: number
  priority: number
  disabled: boolean
  failureCount: number
  isCurrent: boolean
  expiresAt: string | null
  authMethod: string | null
  hasProfileArn: boolean
  email?: string
  refreshTokenHash?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  accountSource?: string
  accountSourceLabel?: string
  kamIdp?: string
  kamProvider?: string
  kamGroupId?: string
  kamGroupName?: string
  labels: string[]
  lastTokenRefreshAt?: string
  lastLivenessCheckAt?: string
}

// 余额响应
export interface BalanceResponse {
  id: number
  subscriptionTitle: string | null
  currentUsage: number
  usageLimit: number
  remaining: number
  usagePercentage: number
  nextResetAt: number | null
  queriedAt?: number | null
  overageEnabled?: boolean
  overageCapable?: boolean
  overageCapabilityRaw?: string
}

export interface BalanceSummaryResponse {
  totalRemaining: number
  totalLimit: number
  queriedCount: number
  totalCount: number
  balances: BalanceResponse[]
  lastUpdatedAt: number | null
}

export interface BalanceAutoRefreshSettings {
  enabled: boolean
  intervalSecs: number
  running: boolean
  lastStartedAt: number | null
  lastFinishedAt: number | null
}

export interface SetBalanceAutoRefreshSettingsRequest {
  enabled?: boolean
  intervalSecs?: number
}

export interface LivenessCheckResponse {
  id: number
  status: string
  checkedAt: string
  latencyMs?: number
  message?: string
}

export interface EnableOverageAllResult {
  enabledIds: number[]
  skippedIds: number[]
  failedIds: number[]
  failureMessages: string[]
}

// 成功响应
export interface SuccessResponse {
  success: boolean
  message: string
}

// 错误响应
export interface AdminErrorResponse {
  error: {
    type: string
    message: string
  }
}

// 凭据事件
export interface CredentialEvent {
  timestamp: string
  eventType: string
  credentialId: number
  statusCode?: number
  bodySnippet?: string
  url?: string
  proxyId?: number
  attempt?: number
  maxRetries?: number
  rpm?: number
  reason?: string
  requestHeaders?: Record<string, string>
  proxyName?: string
  proxyUrl?: string
}

export interface CredentialEventsResponse {
  credentialId: number
  events: CredentialEvent[]
}

// 请求类型
export interface SetDisabledRequest {
  disabled: boolean
}

export interface SetPriorityRequest {
  priority: number
}

// 添加凭据请求
export interface AddCredentialRequest {
  accessToken?: string
  refreshToken: string
  authMethod?: 'social' | 'idc' | 'external_idp'
  clientId?: string
  clientSecret?: string
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
  profileArn?: string
  expiresAt?: string
  priority?: number
  authRegion?: string
  apiRegion?: string
  machineId?: string
  email?: string
  subscriptionTitle?: string
  currentUsage?: number
  usageLimit?: number
  nextResetAt?: number | null
  overageEnabled?: boolean
  overageCapable?: boolean
  overageCapabilityRaw?: string
  accountSource?: string
  accountSourceLabel?: string
  kamIdp?: string
  kamProvider?: string
  kamGroupId?: string
  kamGroupName?: string
  labels?: string[]
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 更新凭据请求
export interface UpdateCredentialRequest {
  refreshToken?: string
  authMethod?: string
  clientId?: string
  clientSecret?: string
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
  authRegion?: string
  apiRegion?: string
  machineId?: string
  email?: string
  accountSource?: string
  accountSourceLabel?: string
  kamIdp?: string
  kamProvider?: string
  kamGroupId?: string
  kamGroupName?: string
  labels?: string[]
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 添加凭据响应
export interface AddCredentialResponse {
  success: boolean
  message: string
  credentialId: number
  email?: string
}

export interface BuilderIdStartResponse {
  sessionId: string
  userCode: string
  verificationUri: string
  interval: number
}

export interface IamSsoStartResponse {
  sessionId: string
  authorizeUrl: string
  expiresIn: number
}

export interface KiroSsoStartResponse {
  sessionId: string
  signInUrl: string
  interval: number
}

export interface AuthFlowPollResponse {
  success: boolean
  completed: boolean
  status?: string
  interval?: number
  account?: AddCredentialResponse
}

export interface SsoTokenImportResponse {
  success: boolean
  accounts: AddCredentialResponse[]
  errors: string[]
}

// API Key 类型
export interface ApiKeyItem {
  id: number
  key: string
  name: string
  enabled: boolean
  createdAt: string
  expiresAt: string | null
  spendingLimit: number | null
  durationDays: number | null
  activatedAt: string | null
}

export interface CreateApiKeyRequest {
  name: string
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
}

export interface UpdateApiKeyRequest {
  name?: string
  enabled?: boolean
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
}

// API Key 用量汇总
export interface UsageSummary {
  apiKeyId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  byModel: ModelUsage[]
}

export interface ModelUsage {
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
}

// RPM 实时监控
export interface RpmSnapshot {
  global: number
  byCredential: Record<string, number>
  byApiKey: Record<string, number>
}

export interface ProxyPoolEntry {
  id: number
  name: string
  url: string
  username?: string
  hasPassword: boolean
  tags: string[]
  enabled: boolean
  healthy: boolean
  lastCheckedAt?: string | null
  latencyMs?: number | null
  exitIp?: string | null
  lastError?: string | null
  consecutiveFailures: number
}

export interface AddProxyRequest {
  name: string
  url: string
  username?: string
  password?: string
  tags?: string[]
}

export interface UpdateProxyRequest {
  name?: string
  url?: string
  username?: string | null
  password?: string | null
  tags?: string[]
}

export interface ProxyBindingEntry {
  proxyId: number
  proxyName: string
  credentials: Array<{ id: number; email?: string | null; disabled: boolean }>
}

export interface SetProxyBindingRequest {
  proxyId?: number | null
  direct?: boolean
}

export interface ExitIpResult {
  name: string
  proxyId?: number | null
  exitIp?: string | null
  error?: string | null
  latencyMs: number
}

export interface CompactionConfig {
  enabled: boolean
  thresholdPercent: number
  preserveRecentPairs: number
  toolResultMaxChars: number
}

export interface KamExportResponse {
  schemaVersion: string
  exportedAt: number
  accounts: unknown[]
}
