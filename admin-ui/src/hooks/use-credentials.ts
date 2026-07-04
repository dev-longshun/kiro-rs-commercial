import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  getCredentials,
  setCredentialDisabled,
  setCredentialPriority,
  resetCredentialFailure,
  getCredentialBalance,
  refreshCredentialToken,
  checkCredentialLiveness,
  setCredentialOverage,
  enableOverageForAllCapable,
  getBalanceSummary,
  refreshAllBalances,
  getBalanceAutoRefreshSettings,
  setBalanceAutoRefreshSettings,
  getCredentialEvents,
  getErrorEvents,
  clearErrorEvents,
  addCredential,
  deleteCredential,
  updateCredential,
  getLoadBalancingMode,
  setLoadBalancingMode,
  getServerInfo,
  getApiKeys,
  createApiKey,
  updateApiKey,
  deleteApiKey,
  getAllUsage,
  resetKeyUsage,
  getRpm,
  getProxyPool,
  addProxy,
  updateProxy,
  deleteProxy,
  setProxyEnabled,
  checkProxy,
  getProxyBindings,
  setProxyBinding,
  rebalanceProxies,
  deleteUnhealthyProxies,
  deleteAllProxies,
  disableHighLatencyProxies,
  checkProxyExitIps,
  exportKam,
  getCompactionConfig,
  setCompactionConfig,
  getAuthKeys,
  setAuthKeys,
  getCacheSimulationConfig,
  setCacheSimulationConfig,
} from '@/api/credentials'
import { storage } from '@/lib/storage'
import type {
  AddCredentialRequest,
  UpdateCredentialRequest,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
  AddProxyRequest,
  UpdateProxyRequest,
  SetBalanceAutoRefreshSettingsRequest,
  SetProxyBindingRequest,
  CompactionConfig,
} from '@/types/api'
import type { CacheSimulationConfig } from '@/api/credentials'

// 查询凭据列表
export function useCredentials() {
  return useQuery({
    queryKey: ['credentials'],
    queryFn: getCredentials,
    refetchInterval: 30000, // 每 30 秒刷新一次
  })
}

// 查询凭据余额
export function useCredentialBalance(id: number | null) {
  return useQuery({
    queryKey: ['credential-balance', id],
    queryFn: () => getCredentialBalance(id!, true),
    enabled: id !== null,
    retry: false, // 余额查询失败时不重试（避免重复请求被封禁的账号）
  })
}

export function useBalanceSummary(refetchInterval = 30000) {
  return useQuery({
    queryKey: ['balance-summary'],
    queryFn: getBalanceSummary,
    refetchInterval,
  })
}

export function useRefreshAllBalances() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: refreshAllBalances,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['balance-summary'] })
      queryClient.invalidateQueries({ queryKey: ['credential-balance'] })
    },
  })
}

export function useBalanceAutoRefreshSettings() {
  return useQuery({
    queryKey: ['balance-auto-refresh-settings'],
    queryFn: getBalanceAutoRefreshSettings,
  })
}

export function useSetBalanceAutoRefreshSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: SetBalanceAutoRefreshSettingsRequest) =>
      setBalanceAutoRefreshSettings(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['balance-auto-refresh-settings'] })
      queryClient.invalidateQueries({ queryKey: ['balance-summary'] })
    },
  })
}

export function useRefreshCredentialToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => refreshCredentialToken(id),
    onSuccess: (_data, id) => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['credential-balance', id] })
    },
  })
}

export function useLivenessCheck() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => checkCredentialLiveness(id),
    onSuccess: (_data, id) => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['credential-balance', id] })
    },
  })
}

export function useSetOverage() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      setCredentialOverage(id, enabled),
    onSuccess: (_data, { id }) => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['balance-summary'] })
      queryClient.invalidateQueries({ queryKey: ['credential-balance', id] })
    },
  })
}

export function useEnableOverageAll() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (ids?: number[]) => enableOverageForAllCapable(ids),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['balance-summary'] })
      queryClient.invalidateQueries({ queryKey: ['credential-balance'] })
    },
  })
}

export function useCredentialEvents(id: number | null) {
  return useQuery({
    queryKey: ['credential-events', id],
    queryFn: () => getCredentialEvents(id!),
    enabled: id !== null,
    refetchInterval: 10000,
  })
}

export function useErrorEvents() {
  return useQuery({
    queryKey: ['errorEvents'],
    queryFn: () => getErrorEvents(500),
    refetchInterval: 10000,
  })
}

export function useClearErrorEvents() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: clearErrorEvents,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['credential-events'] })
      queryClient.invalidateQueries({ queryKey: ['credentialEvents'] })
      queryClient.invalidateQueries({ queryKey: ['errorEvents'] })
    },
  })
}

// 设置禁用状态
export function useSetDisabled() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, disabled }: { id: number; disabled: boolean }) =>
      setCredentialDisabled(id, disabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 设置优先级
export function useSetPriority() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, priority }: { id: number; priority: number }) =>
      setCredentialPriority(id, priority),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 重置失败计数
export function useResetFailure() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => resetCredentialFailure(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 添加新凭据
export function useAddCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: AddCredentialRequest) => addCredential(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 删除凭据
export function useDeleteCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteCredential(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 更新凭据
export function useUpdateCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateCredentialRequest }) =>
      updateCredential(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 获取负载均衡模式
export function useLoadBalancingMode() {
  return useQuery({
    queryKey: ['loadBalancingMode'],
    queryFn: getLoadBalancingMode,
  })
}

// 设置负载均衡模式
export function useSetLoadBalancingMode() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setLoadBalancingMode,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['loadBalancingMode'] })
    },
  })
}

// ============ API Key Hooks ============

// 获取服务器信息
export function useServerInfo() {
  return useQuery({
    queryKey: ['serverInfo'],
    queryFn: getServerInfo,
  })
}

// 查询 API Key 列表
export function useApiKeys() {
  return useQuery({
    queryKey: ['apiKeys'],
    queryFn: getApiKeys,
  })
}

// 创建 API Key
export function useCreateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateApiKeyRequest) => createApiKey(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// 更新 API Key
export function useUpdateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateApiKeyRequest }) =>
      updateApiKey(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// 删除 API Key
export function useDeleteApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteApiKey(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// ============ API Key 用量 Hooks ============

// 查询所有 API Key 用量
export function useAllUsage() {
  return useQuery({
    queryKey: ['apiKeyUsage'],
    queryFn: getAllUsage,
    refetchInterval: 60000, // 每 60 秒刷新
  })
}

// 重置 API Key 用量
export function useResetKeyUsage() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => resetKeyUsage(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeyUsage'] })
    },
  })
}

// ============ RPM 监控 Hooks ============

// 查询实时 RPM 数据（每 5 秒刷新）
export function useRpm() {
  return useQuery({
    queryKey: ['rpm'],
    queryFn: getRpm,
    refetchInterval: 5000,
  })
}

// ============ 代理池 Hooks ============

export function useProxyPool() {
  return useQuery({
    queryKey: ['proxyPool'],
    queryFn: getProxyPool,
    refetchInterval: 30000,
  })
}

export function useAddProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: AddProxyRequest) => addProxy(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
    },
  })
}

export function useUpdateProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateProxyRequest }) =>
      updateProxy(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

export function useDeleteProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteProxy(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

export function useSetProxyEnabled() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      setProxyEnabled(id, enabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
    },
  })
}

export function useCheckProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => checkProxy(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
    },
  })
}

export function useProxyBindings() {
  return useQuery({
    queryKey: ['proxyBindings'],
    queryFn: getProxyBindings,
    refetchInterval: 30000,
  })
}

export function useSetProxyBinding() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ credentialId, ...payload }: { credentialId: number } & SetProxyBindingRequest) =>
      setProxyBinding(credentialId, payload),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
    },
  })
}

export function useRebalanceProxies() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: rebalanceProxies,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
    },
  })
}

export function useDeleteUnhealthyProxies() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deleteUnhealthyProxies,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

export function useDeleteAllProxies() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deleteAllProxies,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

export function useDisableHighLatencyProxies() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (thresholdMs: number) => disableHighLatencyProxies(thresholdMs),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxyPool'] })
      queryClient.invalidateQueries({ queryKey: ['proxyBindings'] })
    },
  })
}

export function useCheckProxyExitIps() {
  return useMutation({
    mutationFn: checkProxyExitIps,
  })
}

export function useExportKam() {
  return useMutation({
    mutationFn: (params?: { enabledOnly?: boolean; ids?: number[] }) => exportKam(params),
  })
}

export function useCompactionConfig() {
  return useQuery({
    queryKey: ['compaction-config'],
    queryFn: getCompactionConfig,
  })
}

export function useSetCompactionConfig() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (config: CompactionConfig) => setCompactionConfig(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['compaction-config'] })
    },
  })
}

// ============ 认证密钥 Hooks ============

export function useAuthKeys() {
  return useQuery({
    queryKey: ['auth-keys'],
    queryFn: getAuthKeys,
  })
}

type AuthKeysQueryData = {
  apiKey: string
  adminApiKey: string
}

type ServerInfoQueryData = {
  masterApiKey: string | null
}

const maskAuthKeyForDisplay = (key: string) => `${key.slice(0, Math.floor(key.length / 2))}***`

export function useSetAuthKeys() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: { apiKey?: string; adminApiKey?: string }) => setAuthKeys(payload),
    onSuccess: (_data, payload) => {
      const nextApiKey = payload.apiKey?.trim()
      const nextAdminApiKey = payload.adminApiKey?.trim()

      if (nextAdminApiKey) {
        storage.setApiKey(nextAdminApiKey)
      }

      queryClient.setQueryData<AuthKeysQueryData | undefined>(['auth-keys'], (current) => ({
        apiKey: nextApiKey ? maskAuthKeyForDisplay(nextApiKey) : current?.apiKey ?? '',
        adminApiKey: nextAdminApiKey ? maskAuthKeyForDisplay(nextAdminApiKey) : current?.adminApiKey ?? '',
      }))

      if (nextApiKey) {
        queryClient.setQueryData<ServerInfoQueryData | undefined>(['serverInfo'], (current) => ({
          ...(current ?? {}),
          masterApiKey: nextApiKey,
        }))
        queryClient.invalidateQueries({ queryKey: ['serverInfo'] })
      }

      queryClient.invalidateQueries({ queryKey: ['auth-keys'] })
    },
  })
}

// ============ 缓存模拟配置 Hooks ============

export function useCacheSimulationConfig() {
  return useQuery({
    queryKey: ['cache-simulation-config'],
    queryFn: getCacheSimulationConfig,
  })
}

export function useSetCacheSimulationConfig() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (config: CacheSimulationConfig) => setCacheSimulationConfig(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cache-simulation-config'] })
    },
  })
}
