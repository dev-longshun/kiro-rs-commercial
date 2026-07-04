import { useMemo, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  Network,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
  Wifi,
  WifiOff,
} from 'lucide-react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  useAddProxy,
  useCheckProxy,
  useCheckProxyExitIps,
  useCredentials,
  useDeleteAllProxies,
  useDeleteProxy,
  useDeleteUnhealthyProxies,
  useDisableHighLatencyProxies,
  useProxyBindings,
  useProxyPool,
  useRebalanceProxies,
  useSetProxyBinding,
  useSetProxyEnabled,
  useUpdateProxy,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { AddProxyRequest, ExitIpResult, ProxyPoolEntry, UpdateProxyRequest } from '@/types/api'

const selectClass =
  'h-9 rounded-sm border-[2.5px] border-border bg-background px-2 text-sm shadow-nb-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'

function splitTags(value: string) {
  return value
    .split(',')
    .map((tag) => tag.trim())
    .filter(Boolean)
}

function formatDate(value?: string | null) {
  if (!value) return '-'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? '-' : date.toLocaleString('zh-CN')
}

function parseProxyInput(raw: string): Pick<AddProxyRequest, 'url' | 'username' | 'password'> | null {
  const value = raw.trim()
  if (!value) return null

  const protocolMatch = value.match(/^(https?|socks5):\/\/(?:([^:]+):([^@]+)@)?(.+)$/)
  if (protocolMatch) {
    const [, protocol, username, password, hostPort] = protocolMatch
    return {
      url: `${protocol}://${hostPort}`,
      username: username || undefined,
      password: password || undefined,
    }
  }

  const parts = value.split(':')
  if (parts.length === 4 && /^\d+$/.test(parts[1])) {
    return {
      url: `http://${parts[0]}:${parts[1]}`,
      username: parts[2],
      password: parts[3],
    }
  }

  const authMatch = value.match(/^([^:]+):([^@]+)@(.+)$/)
  if (authMatch) {
    const [, username, password, hostPort] = authMatch
    return {
      url: `http://${hostPort}`,
      username,
      password,
    }
  }

  if (parts.length === 2 && /^\d+$/.test(parts[1])) {
    return { url: `http://${value}` }
  }

  return { url: value }
}

export function ProxyPoolPanel() {
  const [addOpen, setAddOpen] = useState(false)
  const [editEntry, setEditEntry] = useState<ProxyPoolEntry | null>(null)
  const [addForm, setAddForm] = useState<AddProxyRequest>({ name: '', url: '', tags: [] })
  const [addTags, setAddTags] = useState('')
  const [editForm, setEditForm] = useState<UpdateProxyRequest>({})
  const [editTags, setEditTags] = useState('')
  const [thresholdMs, setThresholdMs] = useState('800')
  const [bindingCredentialId, setBindingCredentialId] = useState('')
  const [bindingTarget, setBindingTarget] = useState('')
  const [exitIpResults, setExitIpResults] = useState<ExitIpResult[] | null>(null)

  const { data: proxies, isLoading } = useProxyPool()
  const { data: bindings } = useProxyBindings()
  const { data: credentials } = useCredentials()
  const addProxy = useAddProxy()
  const updateProxy = useUpdateProxy()
  const deleteProxy = useDeleteProxy()
  const setProxyEnabled = useSetProxyEnabled()
  const checkProxy = useCheckProxy()
  const rebalance = useRebalanceProxies()
  const deleteUnhealthy = useDeleteUnhealthyProxies()
  const deleteAll = useDeleteAllProxies()
  const disableHighLatency = useDisableHighLatencyProxies()
  const checkExitIps = useCheckProxyExitIps()
  const setProxyBinding = useSetProxyBinding()

  const bindingCounts = useMemo(() => {
    const counts = new Map<number, number>()
    bindings?.forEach((entry) => counts.set(entry.proxyId, entry.credentials.length))
    return counts
  }, [bindings])

  const totalCount = proxies?.length ?? 0
  const healthyCount = proxies?.filter((proxy) => proxy.enabled && proxy.healthy).length ?? 0
  const unhealthyCount = proxies?.filter((proxy) => proxy.enabled && !proxy.healthy).length ?? 0
  const disabledCount = proxies?.filter((proxy) => !proxy.enabled).length ?? 0

  const resetAddForm = () => {
    setAddForm({ name: '', url: '', tags: [] })
    setAddTags('')
  }

  const handleProxyUrlInput = (value: string, mode: 'add' | 'edit') => {
    const parsed = parseProxyInput(value)
    if (!parsed) return
    if (mode === 'add') {
      setAddForm((current) => ({ ...current, ...parsed }))
    } else {
      setEditForm((current) => ({ ...current, ...parsed }))
    }
  }

  const handleAdd = () => {
    if (!addForm.name.trim() || !addForm.url.trim()) {
      toast.error('名称和 URL 不能为空')
      return
    }
    addProxy.mutate(
      { ...addForm, tags: splitTags(addTags) },
      {
        onSuccess: () => {
          toast.success('代理已添加')
          setAddOpen(false)
          resetAddForm()
        },
        onError: (error) => toast.error(`添加失败: ${extractErrorMessage(error)}`),
      }
    )
  }

  const openEdit = (entry: ProxyPoolEntry) => {
    setEditEntry(entry)
    setEditForm({
      name: entry.name,
      url: entry.url,
      username: entry.username,
      tags: entry.tags,
    })
    setEditTags(entry.tags.join(', '))
  }

  const handleEdit = () => {
    if (!editEntry) return
    updateProxy.mutate(
      {
        id: editEntry.id,
        data: {
          ...editForm,
          tags: splitTags(editTags),
          password: typeof editForm.password === 'string' && editForm.password.length > 0 ? editForm.password : undefined,
        },
      },
      {
        onSuccess: () => {
          toast.success('代理已更新')
          setEditEntry(null)
        },
        onError: (error) => toast.error(`更新失败: ${extractErrorMessage(error)}`),
      }
    )
  }

  const handleBind = () => {
    const credentialId = Number(bindingCredentialId)
    if (!credentialId) {
      toast.error('请选择凭据')
      return
    }

    const payload =
      bindingTarget === 'direct'
        ? { credentialId, direct: true }
        : bindingTarget
          ? { credentialId, proxyId: Number(bindingTarget) }
          : { credentialId, proxyId: null }

    setProxyBinding.mutate(payload, {
      onSuccess: (res) => toast.success(res.message),
      onError: (error) => toast.error(`绑定失败: ${extractErrorMessage(error)}`),
    })
  }

  const handleCheckExitIps = () => {
    checkExitIps.mutate(undefined, {
      onSuccess: (results) => setExitIpResults(results),
      onError: (error) => toast.error(`出口检测失败: ${extractErrorMessage(error)}`),
    })
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="text-xl font-bold tracking-tight">代理管理</h2>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onClick={handleCheckExitIps} disabled={checkExitIps.isPending}>
            <Network className="h-4 w-4" />
            出口 IP
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              rebalance.mutate(undefined, {
                onSuccess: (res) => toast.success(res.message),
                onError: (error) => toast.error(`分配失败: ${extractErrorMessage(error)}`),
              })
            }}
            disabled={rebalance.isPending || healthyCount === 0}
          >
            <RotateCcw className="h-4 w-4" />
            自动分配
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <Plus className="h-4 w-4" />
            添加代理
          </Button>
        </div>
      </div>

      <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))' }}>
        <div className="nb-card">
          <div className="nb-label text-foreground/60">代理总数</div>
          <div className="text-3xl font-bold tracking-tight mono">{totalCount}</div>
        </div>
        <div className="nb-card">
          <div className="nb-label text-foreground/60">健康</div>
          <div className="text-3xl font-bold tracking-tight mono text-nb-green">{healthyCount}</div>
        </div>
        <div className="nb-card">
          <div className="nb-label text-foreground/60">异常</div>
          <div className="text-3xl font-bold tracking-tight mono text-nb-red">{unhealthyCount}</div>
        </div>
        <div className="nb-card">
          <div className="nb-label text-foreground/60">已禁用</div>
          <div className="text-3xl font-bold tracking-tight mono text-foreground/50">{disabledCount}</div>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            if (!confirm(`确定删除 ${unhealthyCount} 个异常代理吗？`)) return
            deleteUnhealthy.mutate(undefined, {
              onSuccess: (res) => toast.success(`已删除 ${res.deleted} 个异常代理`),
              onError: (error) => toast.error(`删除失败: ${extractErrorMessage(error)}`),
            })
          }}
          disabled={unhealthyCount === 0 || deleteUnhealthy.isPending}
        >
          <AlertTriangle className="h-4 w-4" />
          删除异常
        </Button>
        <div className="flex items-center gap-2">
          <Input
            type="number"
            min="1"
            value={thresholdMs}
            onChange={(event) => setThresholdMs(event.target.value)}
            className="h-9 w-24"
          />
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              const threshold = Number(thresholdMs)
              if (!Number.isFinite(threshold) || threshold <= 0) {
                toast.error('请输入有效延迟阈值')
                return
              }
              disableHighLatency.mutate(threshold, {
                onSuccess: (res) => toast.success(`已禁用 ${res.disabled} 个高延迟代理`),
                onError: (error) => toast.error(`禁用失败: ${extractErrorMessage(error)}`),
              })
            }}
            disabled={disableHighLatency.isPending}
          >
            高延迟禁用
          </Button>
        </div>
        <Button
          size="sm"
          variant="destructive"
          onClick={() => {
            if (!confirm(`确定删除全部 ${totalCount} 个代理吗？`)) return
            deleteAll.mutate(undefined, {
              onSuccess: (res) => toast.success(`已删除 ${res.deleted} 个代理`),
              onError: (error) => toast.error(`删除失败: ${extractErrorMessage(error)}`),
            })
          }}
          disabled={totalCount === 0 || deleteAll.isPending}
        >
          <Trash2 className="h-4 w-4" />
          清空代理
        </Button>
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="space-y-3">
          {proxies?.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">暂无代理</CardContent>
            </Card>
          ) : (
            proxies?.map((proxy) => (
              <div key={proxy.id} className="border-[2.5px] border-border bg-card rounded-sm p-4 shadow-nb-sm">
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div className="min-w-0 space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-xs text-muted-foreground">#{proxy.id}</span>
                      <span className="font-semibold">{proxy.name}</span>
                      {proxy.enabled ? (
                        proxy.healthy ? <Badge variant="success">健康</Badge> : <Badge variant="destructive">异常</Badge>
                      ) : (
                        <Badge variant="secondary">禁用</Badge>
                      )}
                      {proxy.hasPassword && <Badge variant="outline">认证</Badge>}
                      {(bindingCounts.get(proxy.id) ?? 0) > 0 && (
                        <Badge variant="info">绑定 {bindingCounts.get(proxy.id)} 个</Badge>
                      )}
                    </div>
                    <div className="break-all font-mono text-xs text-muted-foreground">{proxy.url}</div>
                    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                      <span>延迟: {proxy.latencyMs != null ? `${proxy.latencyMs}ms` : '-'}</span>
                      <span>出口: {proxy.exitIp || '-'}</span>
                      <span>检测: {formatDate(proxy.lastCheckedAt)}</span>
                      {proxy.lastError && <span className="text-nb-red">错误: {proxy.lastError}</span>}
                    </div>
                    {proxy.tags.length > 0 && (
                      <div className="flex flex-wrap gap-1">
                        {proxy.tags.map((tag) => (
                          <Badge key={tag} variant="outline" className="px-2 py-0 text-[10px] normal-case">
                            {tag}
                          </Badge>
                        ))}
                      </div>
                    )}
                  </div>
                  <div className="flex shrink-0 flex-wrap items-center gap-2">
                    <Switch
                      checked={proxy.enabled}
                      onCheckedChange={() => {
                        setProxyEnabled.mutate(
                          { id: proxy.id, enabled: !proxy.enabled },
                          { onError: (error) => toast.error(`操作失败: ${extractErrorMessage(error)}`) }
                        )
                      }}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        checkProxy.mutate(proxy.id, {
                          onSuccess: (result) => {
                            toast[result.healthy ? 'success' : 'error'](
                              result.healthy ? `检测通过 ${result.latencyMs ?? '-'}ms` : '检测失败'
                            )
                          },
                          onError: (error) => toast.error(`检测失败: ${extractErrorMessage(error)}`),
                        })
                      }}
                      disabled={checkProxy.isPending}
                    >
                      {proxy.healthy ? <Wifi className="h-4 w-4" /> : <WifiOff className="h-4 w-4" />}
                      检测
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => openEdit(proxy)}>
                      <Pencil className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="destructive"
                      onClick={() => {
                        if (!confirm(`确定删除代理 "${proxy.name}" 吗？`)) return
                        deleteProxy.mutate(proxy.id, {
                          onSuccess: (res) => toast.success(res.message),
                          onError: (error) => toast.error(`删除失败: ${extractErrorMessage(error)}`),
                        })
                      }}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </div>
            ))
          )}
        </div>

        <div className="space-y-4">
          <div className="border-[2.5px] border-border bg-card rounded-sm p-4 shadow-nb-sm">
            <h3 className="mb-3 text-sm font-semibold">凭据绑定</h3>
            <div className="space-y-3">
              <select
                className={`${selectClass} w-full`}
                value={bindingCredentialId}
                onChange={(event) => setBindingCredentialId(event.target.value)}
              >
                <option value="">选择凭据</option>
                {credentials?.credentials.map((credential) => (
                  <option key={credential.id} value={credential.id}>
                    #{credential.id} {credential.email || '未命名'} {credential.hasProxy ? `(${credential.proxyUrl})` : ''}
                  </option>
                ))}
              </select>
              <select
                className={`${selectClass} w-full`}
                value={bindingTarget}
                onChange={(event) => setBindingTarget(event.target.value)}
              >
                <option value="">使用全局代理</option>
                <option value="direct">直连</option>
                {proxies?.map((proxy) => (
                  <option key={proxy.id} value={proxy.id}>
                    #{proxy.id} {proxy.name}
                  </option>
                ))}
              </select>
              <Button className="w-full" size="sm" onClick={handleBind} disabled={setProxyBinding.isPending}>
                保存绑定
              </Button>
            </div>
          </div>

          <div className="border-[2.5px] border-border bg-card rounded-sm p-4 shadow-nb-sm">
            <h3 className="mb-3 text-sm font-semibold">绑定概览</h3>
            <div className="space-y-2 text-sm">
              {bindings?.length === 0 ? (
                <div className="text-muted-foreground">暂无绑定</div>
              ) : (
                bindings?.map((entry) => (
                  <div key={entry.proxyId} className="flex items-center justify-between gap-3 border-b border-border/50 pb-2 last:border-b-0">
                    <span>{entry.proxyName}</span>
                    <span className="font-mono">{entry.credentials.length}</span>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>

      <Dialog open={addOpen} onOpenChange={setAddOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>添加代理</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <Input
              placeholder="名称"
              value={addForm.name}
              onChange={(event) => setAddForm((current) => ({ ...current, name: event.target.value }))}
            />
            <Input
              placeholder="URL / user:pass@host:port / host:port:user:pass"
              value={addForm.url}
              onChange={(event) => handleProxyUrlInput(event.target.value, 'add')}
            />
            <div className="grid gap-2 sm:grid-cols-2">
              <Input
                placeholder="用户名"
                value={addForm.username || ''}
                onChange={(event) => setAddForm((current) => ({ ...current, username: event.target.value }))}
              />
              <Input
                type="password"
                placeholder="密码"
                value={addForm.password || ''}
                onChange={(event) => setAddForm((current) => ({ ...current, password: event.target.value }))}
              />
            </div>
            <Input placeholder="标签，逗号分隔" value={addTags} onChange={(event) => setAddTags(event.target.value)} />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddOpen(false)}>取消</Button>
            <Button onClick={handleAdd} disabled={addProxy.isPending}>添加</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(editEntry)} onOpenChange={(open) => !open && setEditEntry(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>编辑代理 #{editEntry?.id}</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <Input
              placeholder="名称"
              value={editForm.name || ''}
              onChange={(event) => setEditForm((current) => ({ ...current, name: event.target.value }))}
            />
            <Input
              placeholder="URL"
              value={editForm.url || ''}
              onChange={(event) => handleProxyUrlInput(event.target.value, 'edit')}
            />
            <div className="grid gap-2 sm:grid-cols-2">
              <Input
                placeholder="用户名"
                value={editForm.username ?? ''}
                onChange={(event) => setEditForm((current) => ({ ...current, username: event.target.value }))}
              />
              <Input
                type="password"
                placeholder="新密码，留空不改"
                value={editForm.password ?? ''}
                onChange={(event) => setEditForm((current) => ({ ...current, password: event.target.value }))}
              />
            </div>
            <Input placeholder="标签，逗号分隔" value={editTags} onChange={(event) => setEditTags(event.target.value)} />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditEntry(null)}>取消</Button>
            <Button onClick={handleEdit} disabled={updateProxy.isPending}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(exitIpResults)} onOpenChange={(open) => !open && setExitIpResults(null)}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>出口 IP 检测</DialogTitle>
          </DialogHeader>
          <div className="max-h-[60vh] overflow-y-auto space-y-2">
            {exitIpResults?.map((result) => (
              <div key={`${result.proxyId ?? 'direct'}-${result.name}`} className="flex items-center justify-between gap-3 border-b border-border/50 pb-2 text-sm">
                <div className="min-w-0">
                  <div className="font-medium">{result.name}</div>
                  <div className="text-xs text-muted-foreground">{result.exitIp || result.error || '-'}</div>
                </div>
                <div className="flex items-center gap-2">
                  {result.error ? (
                    <WifiOff className="h-4 w-4 text-nb-red" />
                  ) : (
                    <CheckCircle2 className="h-4 w-4 text-nb-green" />
                  )}
                  <span className="font-mono text-xs">{result.latencyMs}ms</span>
                </div>
              </div>
            ))}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setExitIpResults(null)}>关闭</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
