import { Grid3X3, Search, Table2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

export type CredentialViewMode = 'card' | 'table'

interface CredentialFilterBarProps {
  search: string
  onSearchChange: (value: string) => void
  status: string
  onStatusChange: (value: string) => void
  source: string
  onSourceChange: (value: string) => void
  overage: string
  onOverageChange: (value: string) => void
  proxy: string
  onProxyChange: (value: string) => void
  viewMode: CredentialViewMode
  onViewModeChange: (mode: CredentialViewMode) => void
  filteredCount: number
  totalCount: number
}

const selectClass =
  'h-9 rounded-sm border-[2.5px] border-border bg-background px-2 text-sm shadow-nb-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'

export function CredentialFilterBar({
  search,
  onSearchChange,
  status,
  onStatusChange,
  source,
  onSourceChange,
  overage,
  onOverageChange,
  proxy,
  onProxyChange,
  viewMode,
  onViewModeChange,
  filteredCount,
  totalCount,
}: CredentialFilterBarProps) {
  return (
    <div className="flex flex-col gap-3 border-[2.5px] border-border bg-card p-3 rounded-sm shadow-nb-sm">
      <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <div className="relative min-w-0 flex-1">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder="搜索 ID、邮箱、来源、分组、标签、代理"
            className="pl-9"
          />
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            size="sm"
            variant={viewMode === 'card' ? 'default' : 'outline'}
            onClick={() => onViewModeChange('card')}
            title="卡片视图"
          >
            <Grid3X3 className="h-4 w-4" />
          </Button>
          <Button
            size="sm"
            variant={viewMode === 'table' ? 'default' : 'outline'}
            onClick={() => onViewModeChange('table')}
            title="表格视图"
          >
            <Table2 className="h-4 w-4" />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <select className={selectClass} value={status} onChange={(event) => onStatusChange(event.target.value)}>
          <option value="all">全部状态</option>
          <option value="enabled">启用</option>
          <option value="disabled">已禁用</option>
          <option value="failed">异常</option>
          <option value="current">当前</option>
        </select>
        <select className={selectClass} value={source} onChange={(event) => onSourceChange(event.target.value)}>
          <option value="all">全部来源</option>
          <option value="manual">手动/未知</option>
          <option value="kam">KAM</option>
          <option value="builder_id">Builder ID</option>
          <option value="iam_sso">IAM SSO</option>
          <option value="kiro_sso">Kiro SSO</option>
          <option value="sso_token">SSO Token</option>
          <option value="enterprise">企业 IdP</option>
        </select>
        <select className={selectClass} value={overage} onChange={(event) => onOverageChange(event.target.value)}>
          <option value="all">全部超额</option>
          <option value="enabled">已开启</option>
          <option value="capable">可开启</option>
          <option value="disabled">未开启</option>
          <option value="unknown">未知</option>
        </select>
        <select className={selectClass} value={proxy} onChange={(event) => onProxyChange(event.target.value)}>
          <option value="all">全部代理</option>
          <option value="bound">已绑定代理</option>
          <option value="unbound">未绑定代理</option>
        </select>
        <div className="ml-auto text-xs text-muted-foreground">
          {filteredCount}/{totalCount}
        </div>
      </div>
    </div>
  )
}
