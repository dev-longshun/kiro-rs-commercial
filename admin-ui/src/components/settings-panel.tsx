import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useAuthKeys, useSetAuthKeys,
  useCacheSimulationConfig, useSetCacheSimulationConfig,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

export function SettingsPanel() {
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const { data: cacheConfig, isLoading: isLoadingCache } = useCacheSimulationConfig()
  const { mutate: setCacheConfig, isPending: isSettingCache } = useSetCacheSimulationConfig()
  const [apiKeyDraft, setApiKeyDraft] = useState('')
  const [adminApiKeyDraft, setAdminApiKeyDraft] = useState('')
  const [editingApiKey, setEditingApiKey] = useState(false)
  const [editingAdminApiKey, setEditingAdminApiKey] = useState(false)
  const [cacheEnabled, setCacheEnabled] = useState(false)
  const [readRatio, setReadRatio] = useState('20')
  const [creationRatio, setCreationRatio] = useState('10')

  useEffect(() => {
    if (cacheConfig) {
      setCacheEnabled(cacheConfig.enabled)
      setReadRatio(String(Math.round(cacheConfig.readRatio * 100)))
      setCreationRatio(String(Math.round(cacheConfig.creationRatio * 100)))
    }
  }, [cacheConfig])

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-semibold">设置</h2>

      <div className="grid gap-4 md:grid-cols-2">
        {/* 认证密钥 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">认证密钥</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">主 API Key</span>
                {!editingApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setApiKeyDraft(''); setEditingApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 API Key"
                    value={apiKeyDraft}
                    onChange={(e) => setApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!apiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ apiKey: apiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('主 API Key 已更新')
                          setEditingApiKey(false)
                          setApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.apiKey ?? '—'}
                </p>
              )}
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">Admin API Key</span>
                {!editingAdminApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setAdminApiKeyDraft(''); setEditingAdminApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingAdminApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 Admin API Key"
                    value={adminApiKeyDraft}
                    onChange={(e) => setAdminApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!adminApiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ adminApiKey: adminApiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('Admin API Key 已更新，请使用新密钥重新登录')
                          setEditingAdminApiKey(false)
                          setAdminApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingAdminApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.adminApiKey ?? '—'}
                </p>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              修改后立即生效，旧密钥将失效。修改 Admin API Key 后需要用新密钥重新登录。
            </p>
          </CardContent>
        </Card>

        {/* 负载均衡 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">负载均衡</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between py-3">
              <span className="text-sm font-medium">均衡模式</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const newMode = loadBalancingData?.mode === 'priority' ? 'balanced' : 'priority'
                  setLoadBalancingMode(newMode, {
                    onSuccess: () => toast.success(`已切换为${newMode === 'priority' ? '优先级模式' : '均衡负载'}`),
                    onError: (e) => toast.error(extractErrorMessage(e)),
                  })
                }}
                disabled={isLoadingMode || isSettingMode}
              >
                {isLoadingMode ? '加载中...' : loadBalancingData?.mode === 'priority' ? '优先级模式' : '均衡负载'}
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* 缓存模拟 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">缓存模拟</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">启用缓存模拟</span>
              <Switch
                checked={cacheEnabled}
                disabled={isLoadingCache || isSettingCache}
                onCheckedChange={(checked) => {
                  setCacheEnabled(checked)
                  setCacheConfig(
                    {
                      enabled: checked,
                      readRatio: parseFloat(readRatio) / 100,
                      creationRatio: parseFloat(creationRatio) / 100,
                    },
                    {
                      onSuccess: () => toast.success(checked ? '缓存模拟已开启' : '缓存模拟已关闭'),
                      onError: (e) => toast.error(extractErrorMessage(e)),
                    }
                  )
                }}
              />
            </div>
            {cacheEnabled && (
              <div className="space-y-3">
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">读取比例 (%)</label>
                  <Input
                    type="number"
                    min="0"
                    max="100"
                    value={readRatio}
                    onChange={(e) => setReadRatio(e.target.value)}
                    className="text-sm"
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">写入比例 (%)</label>
                  <Input
                    type="number"
                    min="0"
                    max="100"
                    value={creationRatio}
                    onChange={(e) => setCreationRatio(e.target.value)}
                    className="text-sm"
                  />
                </div>
                <Button
                  size="sm"
                  disabled={isSettingCache}
                  onClick={() => {
                    const r = parseFloat(readRatio) / 100
                    const c = parseFloat(creationRatio) / 100
                    if (r + c > 1) {
                      toast.error('读取 + 写入比例之和不能超过 100%')
                      return
                    }
                    setCacheConfig(
                      { enabled: true, readRatio: r, creationRatio: c },
                      {
                        onSuccess: () => toast.success('缓存比例已更新'),
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      }
                    )
                  }}
                >
                  保存比例
                </Button>
              </div>
            )}
            <p className="text-xs text-muted-foreground">
              开启后，响应中的 usage 字段将模拟 cache_read 和 cache_creation 比例。
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}