import { Activity, Loader2, RefreshCw, Wallet, Zap } from 'lucide-react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  useLivenessCheck,
  useRefreshCredentialToken,
  useSetOverage,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { BalanceResponse, CredentialStatusItem } from '@/types/api'

interface CredentialTableProps {
  credentials: CredentialStatusItem[]
  selectedIds: Set<number>
  allSelected: boolean
  onToggleSelect: (id: number) => void
  onToggleSelectAll: () => void
  onViewBalance: (id: number) => void
  balances: Map<number, BalanceResponse>
  loadingBalanceIds: Set<number>
  rpmByCredential?: Record<string, number>
}

function formatDate(value?: string | null) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return '-'
  return date.toLocaleString('zh-CN')
}

function formatUsage(balance?: BalanceResponse) {
  if (!balance) return '-'
  return `${balance.remaining.toFixed(1)} / ${balance.usageLimit.toFixed(1)}`
}

function sourceLabel(credential: CredentialStatusItem) {
  return credential.accountSourceLabel || credential.accountSource || credential.authMethod || 'manual'
}

export function CredentialTable({
  credentials,
  selectedIds,
  allSelected,
  onToggleSelect,
  onToggleSelectAll,
  onViewBalance,
  balances,
  loadingBalanceIds,
  rpmByCredential,
}: CredentialTableProps) {
  const refreshToken = useRefreshCredentialToken()
  const livenessCheck = useLivenessCheck()
  const setOverage = useSetOverage()

  const handleRefreshToken = (id: number) => {
    refreshToken.mutate(id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (error) => toast.error(`刷新失败: ${extractErrorMessage(error)}`),
    })
  }

  const handleLiveness = (id: number) => {
    livenessCheck.mutate(id, {
      onSuccess: (res) => toast.success(res.message || `存活检测: ${res.status}`),
      onError: (error) => toast.error(`检测失败: ${extractErrorMessage(error)}`),
    })
  }

  const handleOverage = (id: number, enabled: boolean) => {
    setOverage.mutate(
      { id, enabled },
      {
        onSuccess: (res) => toast.success(res.message),
        onError: (error) => toast.error(`超额设置失败: ${extractErrorMessage(error)}`),
      }
    )
  }

  return (
    <div className="overflow-x-auto border-[2.5px] border-border bg-card rounded-sm shadow-nb-sm">
      <table className="w-full min-w-[1040px] text-sm">
        <thead className="bg-muted/60">
          <tr className="border-b-[2.5px] border-border text-left">
            <th className="w-10 p-3">
              <Checkbox checked={allSelected} onCheckedChange={onToggleSelectAll} />
            </th>
            <th className="p-3">账号</th>
            <th className="p-3">状态</th>
            <th className="p-3">来源</th>
            <th className="p-3">余额</th>
            <th className="p-3">超额</th>
            <th className="p-3">代理</th>
            <th className="p-3">RPM</th>
            <th className="p-3">最近操作</th>
            <th className="p-3 text-right">操作</th>
          </tr>
        </thead>
        <tbody>
          {credentials.map((credential) => {
            const balance = balances.get(credential.id)
            const isLoadingBalance = loadingBalanceIds.has(credential.id)
            const canToggleOverage = balance?.overageCapable === true
            return (
              <tr key={credential.id} className="border-b border-border/60 last:border-b-0">
                <td className="p-3">
                  <Checkbox
                    checked={selectedIds.has(credential.id)}
                    onCheckedChange={() => onToggleSelect(credential.id)}
                  />
                </td>
                <td className="p-3">
                  <div className="flex flex-col gap-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-xs text-muted-foreground">#{credential.id}</span>
                      <span className="font-medium">{credential.email || '未命名凭据'}</span>
                    </div>
                    <div className="flex flex-wrap gap-1">
                      {credential.labels?.slice(0, 3).map((label) => (
                        <Badge key={label} variant="outline" className="px-2 py-0 text-[10px] normal-case">
                          {label}
                        </Badge>
                      ))}
                    </div>
                  </div>
                </td>
                <td className="p-3">
                  <div className="flex flex-wrap gap-1">
                    {credential.isCurrent && <Badge variant="success">当前</Badge>}
                    {credential.disabled ? (
                      <Badge variant="destructive">禁用</Badge>
                    ) : credential.failureCount > 0 ? (
                      <Badge variant="warning">异常 {credential.failureCount}</Badge>
                    ) : (
                      <Badge variant="secondary">启用</Badge>
                    )}
                  </div>
                </td>
                <td className="p-3">
                  <div className="flex flex-col gap-1">
                    <span>{sourceLabel(credential)}</span>
                    {credential.kamGroupName && (
                      <span className="text-xs text-muted-foreground">{credential.kamGroupName}</span>
                    )}
                  </div>
                </td>
                <td className="p-3">
                  {isLoadingBalance ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <div className="flex flex-col gap-1">
                      <span className="font-mono">{formatUsage(balance)}</span>
                      {balance?.subscriptionTitle && (
                        <span className="text-xs text-muted-foreground">{balance.subscriptionTitle}</span>
                      )}
                    </div>
                  )}
                </td>
                <td className="p-3">
                  {balance?.overageEnabled ? (
                    <Badge variant="info">已开</Badge>
                  ) : canToggleOverage ? (
                    <Badge variant="warning">可开</Badge>
                  ) : balance?.overageCapable === false ? (
                    <Badge variant="secondary">不可开</Badge>
                  ) : (
                    <span className="text-muted-foreground">-</span>
                  )}
                </td>
                <td className="p-3">
                  {credential.hasProxy ? (
                    <span className="font-mono text-xs">{credential.proxyUrl || '已绑定'}</span>
                  ) : (
                    <span className="text-muted-foreground">全局/未绑定</span>
                  )}
                </td>
                <td className="p-3 font-mono text-nb-blue">
                  {rpmByCredential?.[String(credential.id)] ?? 0}
                </td>
                <td className="p-3">
                  <div className="flex flex-col gap-1 text-xs text-muted-foreground">
                    <span>Token: {formatDate(credential.lastTokenRefreshAt)}</span>
                    <span>验活: {formatDate(credential.lastLivenessCheckAt)}</span>
                  </div>
                </td>
                <td className="p-3">
                  <div className="flex justify-end gap-1">
                    <Button size="sm" variant="outline" onClick={() => onViewBalance(credential.id)} title="查看余额">
                      <Wallet className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleRefreshToken(credential.id)}
                      disabled={refreshToken.isPending}
                      title="刷新 Token"
                    >
                      <RefreshCw className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleLiveness(credential.id)}
                      disabled={livenessCheck.isPending}
                      title="存活检测"
                    >
                      <Activity className="h-4 w-4" />
                    </Button>
                    {canToggleOverage && (
                      <Button
                        size="sm"
                        variant={balance?.overageEnabled ? 'secondary' : 'info'}
                        onClick={() => handleOverage(credential.id, !balance?.overageEnabled)}
                        disabled={setOverage.isPending}
                        title={balance?.overageEnabled ? '关闭超额' : '开启超额'}
                      >
                        <Zap className="h-4 w-4" />
                      </Button>
                    )}
                  </div>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
