import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  CredentialsStatusResponse,
  BalanceResponse,
  BalanceSummaryResponse,
  BalanceAutoRefreshSettings,
  SetBalanceAutoRefreshSettingsRequest,
  EnableOverageAllResult,
  LivenessCheckResponse,
  SuccessResponse,
  SetDisabledRequest,
  SetPriorityRequest,
  AddCredentialRequest,
  AddCredentialResponse,
  AuthFlowPollResponse,
  BuilderIdStartResponse,
  IamSsoStartResponse,
  KiroSsoStartResponse,
  SsoTokenImportResponse,
  UpdateCredentialRequest,
  ApiKeyItem,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
  UsageSummary,
  RpmSnapshot,
  CredentialEventsResponse,
  CredentialEvent,
  ProxyPoolEntry,
  ProxyBindingEntry,
  AddProxyRequest,
  UpdateProxyRequest,
  SetProxyBindingRequest,
  ExitIpResult,
  CompactionConfig,
  KamExportResponse,
} from '@/types/api'

// 创建 axios 实例
const api = axios.create({
  baseURL: '/api/admin',
  headers: {
    'Content-Type': 'application/json',
  },
})

// 请求拦截器添加 API Key
api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

// 获取所有凭据状态
export async function getCredentials(): Promise<CredentialsStatusResponse> {
  const { data } = await api.get<CredentialsStatusResponse>('/credentials')
  return data
}

// 设置凭据禁用状态
export async function setCredentialDisabled(
  id: number,
  disabled: boolean
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/disabled`,
    { disabled } as SetDisabledRequest
  )
  return data
}

// 设置凭据优先级
export async function setCredentialPriority(
  id: number,
  priority: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/priority`,
    { priority } as SetPriorityRequest
  )
  return data
}

// 重置失败计数
export async function resetCredentialFailure(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset`)
  return data
}

// 获取凭据余额
export async function getCredentialBalance(id: number, refresh = false): Promise<BalanceResponse> {
  const { data } = await api.get<BalanceResponse>(`/credentials/${id}/balance`, {
    params: refresh ? { refresh: true } : undefined,
  })
  return data
}

// 强制刷新单个凭据 Token
export async function refreshCredentialToken(id: number): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/refresh-token`)
  return data
}

// 单凭据存活检测
export async function checkCredentialLiveness(id: number): Promise<LivenessCheckResponse> {
  const { data } = await api.post<LivenessCheckResponse>(`/credentials/${id}/liveness-check`)
  return data
}

// 设置单个凭据超额
export async function setCredentialOverage(id: number, enabled: boolean): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/overage`, { enabled })
  return data
}

// 批量开启可开启超额的凭据；传 ids 时只处理这些凭据，不传则全局处理
export async function enableOverageForAllCapable(ids?: number[]): Promise<EnableOverageAllResult> {
  const { data } = await api.post<EnableOverageAllResult>(
    '/credentials/overage/enable-all',
    ids && ids.length > 0 ? { ids } : { all: true }
  )
  return data
}

// 获取全局余额汇总
export async function getBalanceSummary(): Promise<BalanceSummaryResponse> {
  const { data } = await api.get<BalanceSummaryResponse>('/balance/summary')
  return data
}

// 刷新所有凭据余额
export async function refreshAllBalances(): Promise<BalanceSummaryResponse> {
  const { data } = await api.post<BalanceSummaryResponse>('/balance/refresh-all')
  return data
}

// 获取余额自动刷新设置
export async function getBalanceAutoRefreshSettings(): Promise<BalanceAutoRefreshSettings> {
  const { data } = await api.get<BalanceAutoRefreshSettings>('/balance/auto-refresh')
  return data
}

// 设置余额自动刷新设置
export async function setBalanceAutoRefreshSettings(
  req: SetBalanceAutoRefreshSettingsRequest
): Promise<BalanceAutoRefreshSettings> {
  const { data } = await api.put<BalanceAutoRefreshSettings>('/balance/auto-refresh', req)
  return data
}

// 获取凭据事件日志
export async function getCredentialEvents(id: number): Promise<CredentialEventsResponse> {
  const { data } = await api.get<CredentialEventsResponse>(`/credentials/${id}/events`)
  return data
}

// 获取全局错误日志
export async function getErrorEvents(limit = 200): Promise<{ events: CredentialEvent[]; total: number }> {
  const { data } = await api.get<{ events: CredentialEvent[]; total: number }>('/credentials/error-events', {
    params: { limit },
  })
  return data
}

// 清理全局错误日志
export async function clearErrorEvents(): Promise<{ success: boolean; removed: number }> {
  const { data } = await api.delete<{ success: boolean; removed: number }>('/credentials/error-events')
  return data
}

// 添加新凭据
export async function addCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials', req)
  return data
}

// ============ 登录/导入流程 ============

export async function startBuilderIdLogin(region?: string): Promise<BuilderIdStartResponse> {
  const { data } = await api.post<BuilderIdStartResponse>('/auth/builderid/start', { region })
  return data
}

export async function pollBuilderIdLogin(sessionId: string): Promise<AuthFlowPollResponse> {
  const { data } = await api.post<AuthFlowPollResponse>('/auth/builderid/poll', { sessionId })
  return data
}

export async function startIamSsoLogin(startUrl: string, region?: string): Promise<IamSsoStartResponse> {
  const { data } = await api.post<IamSsoStartResponse>('/auth/iam-sso/start', { startUrl, region })
  return data
}

export async function completeIamSsoLogin(
  sessionId: string,
  callbackUrl: string
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/auth/iam-sso/complete', {
    sessionId,
    callbackUrl,
  })
  return data
}

export async function startKiroSsoLogin(region?: string): Promise<KiroSsoStartResponse> {
  const { data } = await api.post<KiroSsoStartResponse>('/auth/kiro-sso/start', { region })
  return data
}

export async function pollKiroSsoLogin(sessionId: string): Promise<AuthFlowPollResponse> {
  const { data } = await api.post<AuthFlowPollResponse>('/auth/kiro-sso/poll', { sessionId })
  return data
}

export async function cancelKiroSsoLogin(sessionId: string): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>('/auth/kiro-sso/cancel', { sessionId })
  return data
}

export async function importSsoToken(
  bearerToken: string,
  region?: string
): Promise<SsoTokenImportResponse> {
  const { data } = await api.post<SsoTokenImportResponse>('/auth/sso-token', { bearerToken, region })
  return data
}

// 删除凭据
export async function deleteCredential(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/credentials/${id}`)
  return data
}

// 更新凭据
export async function updateCredential(id: number, req: UpdateCredentialRequest): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>(`/credentials/${id}`, req)
  return data
}

// 获取负载均衡模式
export async function getLoadBalancingMode(): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.get<{ mode: 'priority' | 'balanced' }>('/config/load-balancing')
  return data
}

// 设置负载均衡模式
export async function setLoadBalancingMode(mode: 'priority' | 'balanced'): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.put<{ mode: 'priority' | 'balanced' }>('/config/load-balancing', { mode })
  return data
}

// ============ 服务器信息 ============

// 获取服务器连接信息
export async function getServerInfo(): Promise<{ masterApiKey: string | null }> {
  const { data } = await api.get<{ masterApiKey: string | null }>('/server-info')
  return data
}

// ============ API Key 管理 ============

// 获取所有 API Key
export async function getApiKeys(): Promise<ApiKeyItem[]> {
  const { data } = await api.get<ApiKeyItem[]>('/api-keys')
  return data
}

// 创建 API Key
export async function createApiKey(req: CreateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.post<ApiKeyItem>('/api-keys', req)
  return data
}

// 更新 API Key
export async function updateApiKey(id: number, req: UpdateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.put<ApiKeyItem>(`/api-keys/${id}`, req)
  return data
}

// 删除 API Key
export async function deleteApiKey(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}`)
  return data
}

// ============ API Key 用量 ============

// 获取所有 API Key 用量概览
export async function getAllUsage(): Promise<UsageSummary[]> {
  const { data } = await api.get<UsageSummary[]>('/api-keys/usage')
  return data
}

// 获取单个 API Key 用量
export async function getKeyUsage(id: number): Promise<UsageSummary> {
  const { data } = await api.get<UsageSummary>(`/api-keys/${id}/usage`)
  return data
}

// 重置单个 API Key 用量
export async function resetKeyUsage(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}/usage`)
  return data
}

// ============ RPM 监控 ============

// 获取实时 RPM 数据
export async function getRpm(): Promise<RpmSnapshot> {
  const { data } = await api.get<RpmSnapshot>('/rpm')
  return data
}

// ============ 代理池管理 ============

export async function getProxyPool(): Promise<ProxyPoolEntry[]> {
  const { data } = await api.get<ProxyPoolEntry[]>('/proxy-pool')
  return data
}

export async function addProxy(req: AddProxyRequest): Promise<ProxyPoolEntry> {
  const { data } = await api.post<ProxyPoolEntry>('/proxy-pool', req)
  return data
}

export async function updateProxy(id: number, req: UpdateProxyRequest): Promise<ProxyPoolEntry> {
  const { data } = await api.put<ProxyPoolEntry>(`/proxy-pool/${id}`, req)
  return data
}

export async function deleteProxy(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/proxy-pool/${id}`)
  return data
}

export async function setProxyEnabled(id: number, enabled: boolean): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/proxy-pool/${id}/enabled`, { enabled })
  return data
}

export async function checkProxy(id: number): Promise<ProxyPoolEntry> {
  const { data } = await api.post<ProxyPoolEntry>(`/proxy-pool/${id}/check`)
  return data
}

export async function getProxyBindings(): Promise<ProxyBindingEntry[]> {
  const { data } = await api.get<ProxyBindingEntry[]>('/proxy-pool/bindings')
  return data
}

export async function setProxyBinding(
  credentialId: number,
  req: SetProxyBindingRequest
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${credentialId}/proxy-binding`, req)
  return data
}

export async function rebalanceProxies(): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>('/proxy-pool/rebalance')
  return data
}

export async function deleteUnhealthyProxies(): Promise<{ deleted: number; clearedBindings?: number }> {
  const { data } = await api.post<{ deleted: number; clearedBindings?: number }>('/proxy-pool/delete-unhealthy')
  return data
}

export async function deleteAllProxies(): Promise<{ deleted: number; clearedBindings?: number }> {
  const { data } = await api.post<{ deleted: number; clearedBindings?: number }>('/proxy-pool/delete-all')
  return data
}

export async function disableHighLatencyProxies(
  thresholdMs: number
): Promise<{ disabled: number; thresholdMs: number }> {
  const { data } = await api.post<{ disabled: number; thresholdMs: number }>(
    '/proxy-pool/disable-high-latency',
    { thresholdMs }
  )
  return data
}

export async function checkProxyExitIps(): Promise<ExitIpResult[]> {
  const { data } = await api.get<ExitIpResult[]>('/proxy-pool/check-ip')
  return data
}

// ============ KAM / Compaction ============

export async function exportKam(params?: { enabledOnly?: boolean; ids?: number[] }): Promise<KamExportResponse> {
  const { data } = await api.get<KamExportResponse>('/credentials/export-kam', {
    params: {
      enabledOnly: params?.enabledOnly || undefined,
      ids: params?.ids && params.ids.length > 0 ? params.ids.join(',') : undefined,
    },
  })
  return data
}

export async function getCompactionConfig(): Promise<CompactionConfig> {
  const { data } = await api.get<CompactionConfig>('/config/compaction')
  return data
}

export async function setCompactionConfig(config: CompactionConfig): Promise<CompactionConfig> {
  const { data } = await api.put<CompactionConfig>('/config/compaction', config)
  return data
}

// ============ 认证密钥管理 ============

export async function getAuthKeys(): Promise<{ apiKey: string; adminApiKey: string }> {
  const { data } = await api.get<{ apiKey: string; adminApiKey: string }>('/config/auth-keys')
  return data
}

export async function setAuthKeys(payload: { apiKey?: string; adminApiKey?: string }): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/auth-keys', payload)
  return data
}

// ============ 缓存模拟配置 ============

export interface CacheSimulationConfig {
  enabled: boolean
  readRatio: number
  creationRatio: number
}

export async function getCacheSimulationConfig(): Promise<CacheSimulationConfig> {
  const { data } = await api.get<CacheSimulationConfig>('/config/cache-simulation')
  return data
}

export async function setCacheSimulationConfig(config: CacheSimulationConfig): Promise<CacheSimulationConfig> {
  const { data } = await api.put<CacheSimulationConfig>('/config/cache-simulation', config)
  return data
}
