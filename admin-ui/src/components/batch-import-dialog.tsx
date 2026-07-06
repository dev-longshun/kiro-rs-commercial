import { useState } from 'react'
import { toast } from 'sonner'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials, useAddCredential, useDeleteCredential } from '@/hooks/use-credentials'
import { getCredentialBalance, setCredentialDisabled } from '@/api/credentials'
import { extractMicrosoftIssuerUrl, normalizeExpiresAt, resolveExternalIdpMetadata } from '@/lib/external-idp'
import { extractErrorMessage } from '@/lib/utils'

interface BatchImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface CredentialInput {
  accessToken?: string
  refreshToken: string
  clientId?: string
  clientSecret?: string
  tokenEndpoint?: string
  token_endpoint?: string
  issuerUrl?: string
  issuer_url?: string
  scopes?: string
  scope?: string
  profileArn?: string
  profile_arn?: string
  expiresAt?: string | number
  expires_at?: string | number
  authMethod?: string
  provider?: string
  idp?: string
  userId?: string
  email?: string
  region?: string
  authRegion?: string
  apiRegion?: string
  priority?: number
  machineId?: string
  subscriptionTitle?: string
  currentUsage?: number
  usageLimit?: number
  nextResetAt?: number
  overageEnabled?: boolean
  overageCapable?: boolean
  overageCapabilityRaw?: string
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'duplicate' | 'failed'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}

async function sha256Hex(value: string): Promise<string | null> {
  try {
    const subtle = globalThis.crypto?.subtle
    if (!subtle) return null

    const encoded = new TextEncoder().encode(value)
    const digest = await subtle.digest('SHA-256', encoded)
    const bytes = new Uint8Array(digest)
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
  } catch {
    return null
  }
}

function firstNonEmptyString(...values: Array<unknown>): string | undefined {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim()
    }
  }
  return undefined
}

function textIncludesAny(value: string | undefined, keywords: string[]): boolean {
  if (!value) return false
  const lower = value.toLowerCase()
  return keywords.some(keyword => lower.includes(keyword.toLowerCase()))
}

function isBalanceSoftError(error: unknown): boolean {
  const message = extractErrorMessage(error).toLowerCase()
  return (
    message.includes('403') ||
    message.includes('forbidden') ||
    message.includes('not authorized') ||
    message.includes('权限不足') ||
    message.includes('无法获取使用额度') ||
    message.includes('profilearn') ||
    message.includes('accessdenied')
  )
}

function finiteNumber(value: unknown): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value)) return undefined
  return value
}

function importedUsageText(cred: CredentialInput): string {
  const currentUsage = finiteNumber(cred.currentUsage)
  const usageLimit = finiteNumber(cred.usageLimit)
  if (currentUsage !== undefined && usageLimit !== undefined) {
    return `${currentUsage}/${usageLimit}（导入快照）`
  }
  if (usageLimit !== undefined) {
    return `0/${usageLimit}（导入快照）`
  }
  return '已导入，余额稍后同步'
}

export function BatchImportDialog({ open, onOpenChange }: BatchImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])

  const { data: existingCredentials } = useCredentials()
  const { mutateAsync: addCredential } = useAddCredential()
  const { mutateAsync: deleteCredential } = useDeleteCredential()

  const rollbackCredential = async (id: number): Promise<{ success: boolean; error?: string }> => {
    try {
      await setCredentialDisabled(id, true)
    } catch (error) {
      return {
        success: false,
        error: `禁用失败: ${extractErrorMessage(error)}`,
      }
    }

    try {
      await deleteCredential(id)
      return { success: true }
    } catch (error) {
      return {
        success: false,
        error: `删除失败: ${extractErrorMessage(error)}`,
      }
    }
  }

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  const handleBatchImport = async () => {
    try {
      // 1. 解析 JSON
      const parsed = JSON.parse(jsonInput)
      let credentials: CredentialInput[] = Array.isArray(parsed) ? parsed : [parsed]

      if (credentials.length === 0) {
        toast.error('没有可导入的凭据')
        return
      }

      setImporting(true)
      setProgress({ current: 0, total: credentials.length })

      // 2. 初始化结果
      const initialResults: VerificationResult[] = credentials.map((_, i) => ({
        index: i + 1,
        status: 'pending'
      }))
      setResults(initialResults)

      // 3. 检测重复
      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      let successCount = 0
      let duplicateCount = 0
      let failCount = 0
      let rollbackSuccessCount = 0
      let rollbackFailedCount = 0
      let rollbackSkippedCount = 0

      // 4. 导入并验活
      for (let i = 0; i < credentials.length; i++) {
        const cred = credentials[i]
        const token = cred.refreshToken.trim()
        const tokenHash = await sha256Hex(token)

        // 更新状态为检查中
        setCurrentProcessing(`正在处理凭据 ${i + 1}/${credentials.length}`)
        setResults(prev => {
          const newResults = [...prev]
          newResults[i] = { ...newResults[i], status: 'checking' }
          return newResults
        })

        // 检查重复
        if (tokenHash && existingTokenHashes.has(tokenHash)) {
          duplicateCount++
          const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === tokenHash)
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'duplicate',
              error: '该凭据已存在',
              email: existingCred?.email || undefined
            }
            return newResults
          })
          setProgress({ current: i + 1, total: credentials.length })
          continue
        }

        // 更新状态为验活中
        setResults(prev => {
          const newResults = [...prev]
          newResults[i] = { ...newResults[i], status: 'verifying' }
          return newResults
        })

        let addedCredId: number | null = null

        try {
          // 添加凭据
          const clientId = cred.clientId?.trim() || undefined
          const clientSecret = cred.clientSecret?.trim() || undefined
          const explicitTokenEndpoint = firstNonEmptyString(cred.tokenEndpoint, cred.token_endpoint)
          const explicitIssuerUrl = firstNonEmptyString(cred.issuerUrl, cred.issuer_url)
          const inferredIssuerUrl = extractMicrosoftIssuerUrl(explicitIssuerUrl, cred.userId)
          const explicitScopes = firstNonEmptyString(cred.scopes, cred.scope)
          const rawAuthMethod = cred.authMethod?.trim()
          const provider = cred.provider || cred.idp
          const exportedRegion = cred.region?.trim() || undefined
          const isExternalIdp =
            textIncludesAny(rawAuthMethod, ['external_idp', 'external-idp', 'externalidp']) ||
            Boolean(explicitTokenEndpoint || inferredIssuerUrl) ||
            textIncludesAny(provider, ['external', 'microsoft', 'entra', 'azure'])
          const authMethod = isExternalIdp ? 'external_idp' : clientId && clientSecret ? 'idc' : 'social'
          const externalIdpMetadata = authMethod === 'external_idp'
            ? resolveExternalIdpMetadata({
              clientId,
              tokenEndpoint: explicitTokenEndpoint,
              issuerUrl: explicitIssuerUrl,
              scopes: explicitScopes,
              userId: cred.userId,
            })
            : {}

          if (authMethod === 'external_idp' && (!clientId || !externalIdpMetadata.tokenEndpoint)) {
            throw new Error('external_idp 模式需要同时提供 clientId 和 tokenEndpoint')
          }

          // idc 模式下必须同时提供 clientId 和 clientSecret；external_idp 是 public client，不需要 clientSecret
          if (authMethod === 'social' && (clientId || clientSecret || explicitTokenEndpoint)) {
            throw new Error('idc 模式需要同时提供 clientId 和 clientSecret')
          }

          const addedCred = await addCredential({
            accessToken: authMethod === 'external_idp' ? cred.accessToken?.trim() || undefined : undefined,
            refreshToken: token,
            authMethod,
            authRegion: cred.authRegion?.trim() || exportedRegion,
            apiRegion: cred.apiRegion?.trim() || exportedRegion,
            clientId,
            clientSecret: authMethod === 'idc' ? clientSecret : undefined,
            tokenEndpoint: authMethod === 'external_idp' ? externalIdpMetadata.tokenEndpoint : undefined,
            issuerUrl: authMethod === 'external_idp' ? externalIdpMetadata.issuerUrl : undefined,
            scopes: authMethod === 'external_idp' ? externalIdpMetadata.scopes : undefined,
            profileArn: firstNonEmptyString(cred.profileArn, cred.profile_arn),
            expiresAt: authMethod === 'external_idp'
              ? normalizeExpiresAt(firstNonEmptyString(cred.expiresAt, cred.expires_at) ?? cred.expiresAt ?? cred.expires_at)
              : undefined,
            priority: cred.priority || 0,
            machineId: cred.machineId?.trim() || undefined,
            email: cred.email?.trim() || undefined,
            subscriptionTitle: firstNonEmptyString(cred.subscriptionTitle),
            currentUsage: finiteNumber(cred.currentUsage),
            usageLimit: finiteNumber(cred.usageLimit),
            nextResetAt: finiteNumber(cred.nextResetAt),
            overageEnabled: cred.overageEnabled,
            overageCapable: cred.overageCapable,
            overageCapabilityRaw: firstNonEmptyString(cred.overageCapabilityRaw),
          })

          addedCredId = addedCred.credentialId

          // 延迟 1 秒
          await new Promise(resolve => setTimeout(resolve, 1000))

          // 验活
          let balance
          try {
            balance = await getCredentialBalance(addedCred.credentialId)
          } catch (balanceError) {
            if (!isBalanceSoftError(balanceError)) {
              throw balanceError
            }

            successCount++
            if (tokenHash) existingTokenHashes.add(tokenHash)
            setCurrentProcessing(addedCred.email ? `导入成功: ${addedCred.email}` : `导入成功: 凭据 ${i + 1}`)
            setResults(prev => {
              const newResults = [...prev]
              newResults[i] = {
                ...newResults[i],
                status: 'verified',
                usage: importedUsageText(cred),
                email: addedCred.email || undefined,
                credentialId: addedCred.credentialId
              }
              return newResults
            })
            setProgress({ current: i + 1, total: credentials.length })
            continue
          }

          // 验活成功
          successCount++
          if (tokenHash) existingTokenHashes.add(tokenHash)
          setCurrentProcessing(addedCred.email ? `验活成功: ${addedCred.email}` : `验活成功: 凭据 ${i + 1}`)
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'verified',
              usage: `${balance.currentUsage}/${balance.usageLimit}`,
              email: addedCred.email || undefined,
              credentialId: addedCred.credentialId
            }
            return newResults
          })
        } catch (error) {
          // 验活失败，尝试回滚（先禁用再删除）
          let rollbackStatus: VerificationResult['rollbackStatus'] = 'skipped'
          let rollbackError: string | undefined

          if (addedCredId) {
            const rollbackResult = await rollbackCredential(addedCredId)
            if (rollbackResult.success) {
              rollbackStatus = 'success'
              rollbackSuccessCount++
            } else {
              rollbackStatus = 'failed'
              rollbackFailedCount++
              rollbackError = rollbackResult.error
            }
          } else {
            rollbackSkippedCount++
          }

          failCount++
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'failed',
              error: extractErrorMessage(error),
              email: undefined,
              rollbackStatus,
              rollbackError,
            }
            return newResults
          })
        }

        setProgress({ current: i + 1, total: credentials.length })
      }

      // 显示结果
      if (failCount === 0 && duplicateCount === 0) {
        toast.success(`成功导入并验活 ${successCount} 个凭据`)
      } else {
        const failureSummary = failCount > 0
          ? `，失败 ${failCount} 个（已排除 ${rollbackSuccessCount}，未排除 ${rollbackFailedCount}，无需排除 ${rollbackSkippedCount}）`
          : ''
        toast.info(`验活完成：成功 ${successCount} 个，重复 ${duplicateCount} 个${failureSummary}`)

        if (rollbackFailedCount > 0) {
          toast.warning(`有 ${rollbackFailedCount} 个失败凭据回滚未完成，请手动禁用并删除`)
        }
      }
    } catch (error) {
      toast.error('JSON 格式错误: ' + extractErrorMessage(error))
    } finally {
      setImporting(false)
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-border" />
      case 'checking':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-nb-blue" />
      case 'verified':
        return <CheckCircle2 className="w-5 h-5 text-nb-green" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-nb-yellow" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-nb-red" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending':
        return '等待中'
      case 'checking':
        return '检查重复...'
      case 'verifying':
        return '验活中...'
      case 'verified':
        return '验活成功'
      case 'duplicate':
        return '重复凭据'
      case 'failed':
        if (result.rollbackStatus === 'success') return '验活失败（已排除）'
        if (result.rollbackStatus === 'failed') return '验活失败（未排除）'
        return '验活失败（未创建）'
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(newOpen) => {
        // 关闭时清空表单（但不在导入过程中清空）
        if (!newOpen && !importing) {
          resetForm()
        }
        onOpenChange(newOpen)
      }}
    >
      <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>批量导入凭据（自动验活）</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">
              JSON 格式凭据
            </label>
            <textarea
              placeholder={'粘贴 JSON 格式的凭据（支持单个对象或数组）\n例如: [{"refreshToken":"...","clientId":"...","clientSecret":"...","authRegion":"us-east-1","apiRegion":"us-west-2"}]\n支持 region 字段自动映射为 authRegion'}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
              className="flex min-h-[200px] w-full border-[2.5px] border-border bg-background px-3 py-2 text-sm rounded-sm placeholder:text-foreground/40 focus-visible:outline-none focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50 font-mono"
            />
            <p className="text-xs text-muted-foreground">
              💡 导入时自动验活，失败的凭据会被排除
            </p>
          </div>

          {(importing || results.length > 0) && (
            <>
              {/* 进度条 */}
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? '验活进度' : '验活完成'}</span>
                  <span>{progress.current} / {progress.total}</span>
                </div>
                <div className="w-full bg-muted border border-border rounded-sm h-2">
                  <div
                    className="bg-primary h-2 rounded-sm transition-all"
                    style={{ width: `${(progress.current / progress.total) * 100}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">
                    {currentProcessing}
                  </div>
                )}
              </div>

              {/* 统计 */}
              <div className="flex gap-4 text-sm">
                <span className="text-nb-green">
                  ✓ 成功: {results.filter(r => r.status === 'verified').length}
                </span>
                <span className="text-nb-yellow">
                  ⚠ 重复: {results.filter(r => r.status === 'duplicate').length}
                </span>
                <span className="text-nb-red">
                  ✗ 失败: {results.filter(r => r.status === 'failed').length}
                </span>
              </div>

              {/* 结果列表 */}
              <div className="border-[2.5px] border-border rounded-sm divide-y divide-border max-h-[300px] overflow-y-auto">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || `凭据 #${result.index}`}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">
                            用量: {result.usage}
                          </div>
                        )}
                        {result.error && (
                          <div className="text-xs text-nb-red mt-1">
                            {result.error}
                          </div>
                        )}
                        {result.rollbackError && (
                          <div className="text-xs text-nb-red mt-1">
                            回滚失败: {result.rollbackError}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              onOpenChange(false)
              resetForm()
            }}
            disabled={importing}
          >
            {importing ? '验活中...' : results.length > 0 ? '关闭' : '取消'}
          </Button>
          {results.length === 0 && (
            <Button
              type="button"
              onClick={handleBatchImport}
              disabled={importing || !jsonInput.trim()}
            >
              开始导入并验活
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
