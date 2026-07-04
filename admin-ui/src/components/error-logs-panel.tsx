import { useState } from 'react'
import { Trash2, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { useErrorEvents, useClearErrorEvents } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialEvent } from '@/types/api'
import { useQueryClient } from '@tanstack/react-query'

const EVENT_TYPE_CONFIG: Record<string, { label: string; color: string }> = {
  api_failure: { label: '请求失败', color: 'bg-nb-red/10 text-nb-red' },
  rate_limited: { label: '限流', color: 'bg-nb-orange/10 text-nb-orange' },
  quota_exhausted: { label: '额度用尽', color: 'bg-nb-red/10 text-nb-red' },
  token_refresh_failure: { label: 'Token刷新失败', color: 'bg-nb-orange/10 text-nb-orange' },
  network_error: { label: '网络错误', color: 'bg-nb-orange/10 text-nb-orange' },
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

function ErrorEventRow({
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
  const hasDetails = event.statusCode || event.bodySnippet || event.url || event.reason || event.requestHeaders

  return (
    <div className="border rounded-md px-3 py-2 text-sm">
      <div className="flex items-center gap-2 cursor-pointer" onClick={hasDetails ? onToggle : undefined}>
        <span className="text-muted-foreground text-xs shrink-0 w-[130px]">{formatTime(event.timestamp)}</span>
        <Badge variant="secondary" className={`text-xs shrink-0 ${config.color}`}>{config.label}</Badge>
        <span className="text-xs text-muted-foreground">#{event.credentialId}</span>
        {event.statusCode && <span className="text-xs text-muted-foreground">HTTP {event.statusCode}</span>}
        {event.rpm != null && <span className="text-xs font-medium text-nb-orange">RPM: {event.rpm}</span>}
        {event.reason && <span className="text-xs text-muted-foreground truncate flex-1">{event.reason}</span>}
        {event.proxyName && <span className="text-xs text-muted-foreground">{event.proxyName}</span>}
      </div>
      {expanded && hasDetails && (
        <div className="mt-2 pl-[138px] space-y-1 text-xs text-muted-foreground">
          {event.url && <div>URL: <span className="font-mono break-all">{event.url}</span></div>}
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

export function ErrorLogsPanel() {
  const { data, isLoading } = useErrorEvents()
  const { mutate: clearErrors, isPending: isClearing } = useClearErrorEvents()
  const queryClient = useQueryClient()
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null)

  const events = data?.events ?? []

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
        <CardTitle className="text-lg font-semibold">错误日志</CardTitle>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => queryClient.invalidateQueries({ queryKey: ['errorEvents'] })}
          >
            <RefreshCw className="h-4 w-4 mr-1" />
            刷新
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-nb-red hover:text-nb-red hover:bg-nb-red/10"
            onClick={() => clearErrors(undefined, {
              onSuccess: (d) => toast.success(`已清理 ${d.removed} 条错误日志`),
              onError: (e) => toast.error(extractErrorMessage(e)),
            })}
            disabled={isClearing || events.length === 0}
          >
            <Trash2 className="h-4 w-4 mr-1" />
            {isClearing ? '清理中...' : '清理全部'}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="py-8 text-center text-muted-foreground">加载中...</div>
        ) : events.length === 0 ? (
          <div className="py-8 text-center text-muted-foreground">暂无错误日志</div>
        ) : (
          <div className="space-y-1 max-h-[600px] overflow-y-auto">
            {events.map((event, idx) => (
              <ErrorEventRow
                key={idx}
                event={event}
                expanded={expandedIdx === idx}
                onToggle={() => setExpandedIdx(expandedIdx === idx ? null : idx)}
              />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
