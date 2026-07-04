import { useState } from 'react'
import { Download } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { useCredentialEvents } from '@/hooks/use-credentials'
import type { CredentialEvent } from '@/types/api'

interface CredentialEventsDialogProps {
  credentialId: number | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

const EVENT_TYPE_CONFIG: Record<string, { label: string; color: string }> = {
  api_success: { label: '请求成功', color: 'bg-nb-green/10 text-nb-green' },
  api_failure: { label: '请求失败', color: 'bg-nb-red/10 text-nb-red' },
  rate_limited: { label: '限流', color: 'bg-nb-orange/10 text-nb-orange' },
  quota_exhausted: { label: '额度用尽', color: 'bg-nb-red/10 text-nb-red' },
  token_refresh_success: { label: 'Token刷新', color: 'bg-nb-blue/10 text-nb-blue' },
  token_refresh_failure: { label: 'Token刷新失败', color: 'bg-nb-orange/10 text-nb-orange' },
  auto_disabled: { label: '自动禁用', color: 'bg-nb-red/10 text-nb-red' },
  self_healing_recovery: { label: '自愈恢复', color: 'bg-purple-100 text-purple-800' },
  manual_enabled: { label: '手动启用', color: 'bg-nb-green/10 text-nb-green' },
  manual_disabled: { label: '手动禁用', color: 'bg-muted text-foreground' },
  network_error: { label: '网络错误', color: 'bg-nb-orange/10 text-nb-orange' },
  model_fallback: { label: '模型降级', color: 'bg-nb-blue/10 text-nb-blue' },
  pool_failover: { label: '池切换', color: 'bg-muted text-foreground' },
}

function formatTime(ts: string) {
  const d = new Date(ts)
  return d.toLocaleString('zh-CN', {
    hour12: false,
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function EventRow({
  event,
  expanded,
  onToggle,
}: {
  event: CredentialEvent
  expanded: boolean
  onToggle: () => void
}) {
  const config = EVENT_TYPE_CONFIG[event.eventType] ?? {
    label: event.eventType,
    color: 'bg-muted text-foreground',
  }
  const hasDetails =
    event.statusCode ||
    event.bodySnippet ||
    event.url ||
    event.proxyId != null ||
    event.attempt ||
    event.reason ||
    event.requestHeaders ||
    event.proxyName

  return (
    <div className="border rounded-md px-3 py-2 text-sm">
      <div className="flex items-center gap-2 cursor-pointer" onClick={hasDetails ? onToggle : undefined}>
        <span className="text-muted-foreground text-xs shrink-0 w-[130px]">{formatTime(event.timestamp)}</span>
        <Badge variant="secondary" className={`text-xs shrink-0 ${config.color}`}>{config.label}</Badge>
        {event.statusCode && <span className="text-xs text-muted-foreground">HTTP {event.statusCode}</span>}
        {event.reason && <span className="text-xs text-muted-foreground truncate">{event.reason}</span>}
        {event.proxyName && <span className="text-xs text-muted-foreground">{event.proxyName}{event.proxyId != null ? `#${event.proxyId}` : ''}</span>}
        {event.attempt && <span className="text-xs text-muted-foreground">重试 {event.attempt}/{event.maxRetries}</span>}
      </div>
      {expanded && hasDetails && (
        <div className="mt-2 pl-[138px] space-y-1 text-xs text-muted-foreground">
          {event.url && <div>URL: <span className="font-mono break-all">{event.url}</span></div>}
          {event.proxyUrl && <div>代理: <span className="font-mono break-all">{event.proxyName} → {event.proxyUrl}</span></div>}
          {event.rpm != null && <div>RPM: {event.rpm}</div>}
          {event.bodySnippet && (
            <div>
              <span>响应摘要:</span>
              <pre className="mt-1 p-2 bg-muted rounded text-xs whitespace-pre-wrap break-all max-h-[200px] overflow-y-auto">{event.bodySnippet}</pre>
            </div>
          )}
          {event.requestHeaders && (
            <div>
              <span>请求头:</span>
              <pre className="mt-1 p-2 bg-muted rounded text-xs whitespace-pre-wrap break-all max-h-[200px] overflow-y-auto">{Object.entries(event.requestHeaders).map(([k, v]) => `${k}: ${v}`).join('\n')}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

export function CredentialEventsDialog({ credentialId, open, onOpenChange }: CredentialEventsDialogProps) {
  const { data, isLoading } = useCredentialEvents(open ? credentialId : null)
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null)

  const events = data?.events ?? []
  const reversed = [...events].reverse()

  const handleExport = () => {
    const blob = new Blob([JSON.stringify(events, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `credential-${credentialId}-events.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader className="flex flex-row items-center justify-between">
          <DialogTitle>凭据 #{credentialId} 事件日志</DialogTitle>
          <Button size="sm" variant="outline" onClick={handleExport} disabled={events.length === 0}>
            <Download className="h-4 w-4 mr-1" />
            导出 JSON
          </Button>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto min-h-0">
          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground">加载中...</div>
          ) : reversed.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground">暂无事件记录</div>
          ) : (
            <div className="space-y-1">
              {reversed.map((event, idx) => (
                <EventRow
                  key={idx}
                  event={event}
                  expanded={expandedIdx === idx}
                  onToggle={() => setExpandedIdx(expandedIdx === idx ? null : idx)}
                />
              ))}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
