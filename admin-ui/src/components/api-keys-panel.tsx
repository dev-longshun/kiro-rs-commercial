import { useState } from 'react'
import { Copy, Plus, Pencil, Trash2, Key, Check, Clock, BarChart3, RotateCcw } from 'lucide-react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useApiKeys, useCreateApiKey, useUpdateApiKey, useDeleteApiKey, useServerInfo, useAllUsage, useResetKeyUsage } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { ApiKeyItem, UsageSummary } from '@/types/api'

export function ApiKeysPanel() {
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [editingKey, setEditingKey] = useState<ApiKeyItem | null>(null)
  const [newName, setNewName] = useState('')
  const [newDuration, setNewDuration] = useState<number | null>(1) // 天数，null 表示永不过期
  const [editName, setEditName] = useState('')
  const [editDuration, setEditDuration] = useState<number | null>(1)
  const [copiedId, setCopiedId] = useState<number | null>(null)
  const [copiedMaster, setCopiedMaster] = useState(false)
  const [copiedUrl, setCopiedUrl] = useState(false)

  const durationOptions = [
    { label: '1 天', days: 1 },
    { label: '3 天', days: 3 },
    { label: '7 天', days: 7 },
    { label: '30 天', days: 30 },
    { label: '永不过期', days: null as number | null },
  ]

  const calcExpiresAt = (days: number | null): string | null => {
    if (days === null) return null
    const date = new Date()
    date.setDate(date.getDate() + days)
    return date.toISOString()
  }

  const previewExpiry = (days: number | null): string => {
    if (days === null) return '永不过期'
    const date = new Date()
    date.setDate(date.getDate() + days)
    return date.toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }

  const { data: apiKeys, isLoading } = useApiKeys()
  const { data: serverInfo } = useServerInfo()
  const { data: usageData } = useAllUsage()
  const { mutate: createKey, isPending: isCreating } = useCreateApiKey()
  const { mutate: updateKey } = useUpdateApiKey()
  const { mutate: deleteKey } = useDeleteApiKey()
  const { mutate: resetUsage } = useResetKeyUsage()

  // 构建 key_id -> usage 的映射
  const usageMap = new Map<number, UsageSummary>()
  usageData?.forEach((u) => usageMap.set(u.apiKeyId, u))

  const formatTokens = (tokens: number): string => {
    if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`
    if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`
    return tokens.toString()
  }

  const formatCost = (cost: number): string => {
    return `$${cost.toFixed(4)}`
  }

  const handleResetUsage = (key: ApiKeyItem) => {
    if (!confirm(`确定要重置 "${key.name}" 的用量记录吗？`)) return
    resetUsage(key.id, {
      onSuccess: () => toast.success('用量已重置'),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }
  const copyToClipboard = async (text: string, target: 'url' | 'master' | number) => {
    await navigator.clipboard.writeText(text)
    if (target === 'url') {
      setCopiedUrl(true)
      setTimeout(() => setCopiedUrl(false), 2000)
    } else if (target === 'master') {
      setCopiedMaster(true)
      setTimeout(() => setCopiedMaster(false), 2000)
    } else {
      setCopiedId(target)
      setTimeout(() => setCopiedId(null), 2000)
    }
    toast.success('已复制到剪贴板')
  }

  const getKeyStatus = (key: ApiKeyItem): 'active' | 'disabled' | 'expired' => {
    if (!key.enabled) return 'disabled'
    if (key.expiresAt && new Date(key.expiresAt) <= new Date()) return 'expired'
    return 'active'
  }

  const handleCreate = () => {
    createKey(
      {
        name: newName,
        expiresAt: calcExpiresAt(newDuration),
      },
      {
        onSuccess: () => {
          toast.success('API Key 创建成功')
          setCreateDialogOpen(false)
          setNewName('')
          setNewDuration(1)
        },
        onError: (err) => toast.error(`创建失败: ${extractErrorMessage(err)}`),
      }
    )
  }

  const handleUpdate = () => {
    if (!editingKey) return
    updateKey(
      {
        id: editingKey.id,
        data: {
          name: editName || undefined,
          expiresAt: calcExpiresAt(editDuration),
        },
      },
      {
        onSuccess: () => {
          toast.success('已更新')
          setEditingKey(null)
        },
        onError: (err) => toast.error(`更新失败: ${extractErrorMessage(err)}`),
      }
    )
  }

  const handleToggleEnabled = (key: ApiKeyItem) => {
    updateKey(
      { id: key.id, data: { enabled: !key.enabled } },
      {
        onSuccess: () => toast.success(key.enabled ? '已禁用' : '已启用'),
        onError: (err) => toast.error(extractErrorMessage(err)),
      }
    )
  }

  const handleDelete = (key: ApiKeyItem) => {
    if (!confirm(`确定要删除 "${key.name}" 的 API Key 吗？`)) return
    deleteKey(key.id, {
      onSuccess: () => toast.success('已删除'),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }

  const openEdit = (key: ApiKeyItem) => {
    setEditingKey(key)
    setEditName(key.name)
    // 根据剩余时间推算最接近的选项，默认 1 天
    if (!key.expiresAt) {
      setEditDuration(null)
    } else {
      const remaining = Math.max(1, Math.ceil((new Date(key.expiresAt).getTime() - Date.now()) / (1000 * 60 * 60 * 24)))
      const closest = [1, 3, 7, 30].reduce((prev, curr) =>
        Math.abs(curr - remaining) < Math.abs(prev - remaining) ? curr : prev
      )
      setEditDuration(closest)
    }
  }

  const maskKey = (key: string) => key.slice(0, 7) + '...' + key.slice(-4)

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }
  return (
    <div className="space-y-4">
      {/* 服务信息 */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
            <Key className="h-4 w-4" />
            服务连接信息
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-xs text-muted-foreground">API Base URL</div>
              <code className="text-sm">{window.location.origin}</code>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => copyToClipboard(window.location.origin, 'url')}
            >
              {copiedUrl ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <div className="text-xs text-muted-foreground">主 API Key</div>
              <code className="text-sm">{serverInfo?.masterApiKey ? maskKey(serverInfo.masterApiKey) : '加载中...'}</code>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => serverInfo?.masterApiKey && copyToClipboard(serverInfo.masterApiKey, 'master')}
              disabled={!serverInfo?.masterApiKey}
            >
              {copiedMaster ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* API Key 列表 */}
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-semibold">API Key 管理</h2>
        <Button onClick={() => setCreateDialogOpen(true)} size="sm">
          <Plus className="h-4 w-4 mr-2" />
          创建 Key
        </Button>
      </div>
      {isLoading ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">加载中...</CardContent>
        </Card>
      ) : !apiKeys || apiKeys.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">
            暂无用户 API Key，点击"创建 Key"添加
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3">
          {apiKeys.map((apiKey) => {
            const status = getKeyStatus(apiKey)
            const usage = usageMap.get(apiKey.id)
            return (
              <Card key={apiKey.id} className={status !== 'active' ? 'opacity-60' : ''}>
                <CardContent className="py-3 px-4">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3 min-w-0 flex-1">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="font-medium truncate">{apiKey.name}</span>
                          <Badge variant={status === 'active' ? 'success' : status === 'expired' ? 'warning' : 'destructive'}>
                            {status === 'active' ? '启用' : status === 'expired' ? '已过期' : '已禁用'}
                          </Badge>
                        </div>
                        <div className="flex items-center gap-3 mt-1 text-xs text-muted-foreground">
                          <code>{maskKey(apiKey.key)}</code>
                          <span>创建: {formatDate(apiKey.createdAt)}</span>
                          {apiKey.expiresAt && (
                            <span className="flex items-center gap-1">
                              <Clock className="h-3 w-3" />
                              到期: {formatDate(apiKey.expiresAt)}
                            </span>
                          )}
                        </div>
                        {/* 用量信息（始终显示） */}
                        <div className="flex items-center gap-3 mt-1.5 text-xs">
                          <span className="flex items-center gap-1 text-muted-foreground">
                            <BarChart3 className="h-3 w-3" />
                            {usage?.totalRequests ?? 0} 次请求
                          </span>
                          <span className="text-muted-foreground">
                            入 {formatTokens(usage?.totalInputTokens ?? 0)} / 出 {formatTokens(usage?.totalOutputTokens ?? 0)}
                          </span>
                          <span className="font-medium text-orange-600 dark:text-orange-400">
                            {formatCost(usage?.totalCost ?? 0)}
                          </span>
                          {usage && usage.totalRequests > 0 && (
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-5 w-5 p-0 text-muted-foreground hover:text-destructive"
                              onClick={() => handleResetUsage(apiKey)}
                              title="重置用量"
                            >
                              <RotateCcw className="h-3 w-3" />
                            </Button>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-1 ml-2">
                      <Button variant="ghost" size="sm" onClick={() => copyToClipboard(apiKey.key, apiKey.id)} title="复制 Key">
                        {copiedId === apiKey.id ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
                      </Button>
                      <Switch checked={apiKey.enabled} onCheckedChange={() => handleToggleEnabled(apiKey)} />
                      <Button variant="ghost" size="sm" onClick={() => openEdit(apiKey)} title="编辑">
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => handleDelete(apiKey)} title="删除" className="text-destructive hover:text-destructive">
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )
          })}
        </div>
      )}
      {/* 创建对话框 */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>创建 API Key</DialogTitle>
            <DialogDescription>为用户创建一个新的 API Key</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">备注名称</label>
              <Input
                placeholder="如：张三-月付"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
              />
            </div>
            <div>
              <label className="text-sm font-medium">有效期</label>
              <div className="flex flex-wrap gap-2 mt-2">
                {durationOptions.map((opt) => (
                  <Button
                    key={opt.label}
                    type="button"
                    size="sm"
                    variant={newDuration === opt.days ? 'default' : 'outline'}
                    onClick={() => setNewDuration(opt.days)}
                  >
                    {opt.label}
                  </Button>
                ))}
              </div>
              <div className="text-xs text-muted-foreground mt-2">
                <Clock className="h-3 w-3 inline mr-1" />
                到期时间: {previewExpiry(newDuration)}
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>取消</Button>
            <Button onClick={handleCreate} disabled={!newName.trim() || isCreating}>
              {isCreating ? '创建中...' : '创建'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 编辑对话框 */}
      <Dialog open={!!editingKey} onOpenChange={(open) => !open && setEditingKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>编辑 API Key</DialogTitle>
            <DialogDescription>修改备注或续期</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">备注名称</label>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
              />
            </div>
            <div>
              <label className="text-sm font-medium">续期（从现在起）</label>
              <div className="flex flex-wrap gap-2 mt-2">
                {durationOptions.map((opt) => (
                  <Button
                    key={opt.label}
                    type="button"
                    size="sm"
                    variant={editDuration === opt.days ? 'default' : 'outline'}
                    onClick={() => setEditDuration(opt.days)}
                  >
                    {opt.label}
                  </Button>
                ))}
              </div>
              <div className="text-xs text-muted-foreground mt-2">
                <Clock className="h-3 w-3 inline mr-1" />
                到期时间: {previewExpiry(editDuration)}
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditingKey(null)}>取消</Button>
            <Button onClick={handleUpdate}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
