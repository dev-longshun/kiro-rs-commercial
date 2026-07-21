import { useState, useMemo } from 'react'
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

interface KamImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

// KAM 导出 JSON 中的账号结构
interface KamAccount {
  email?: string
  userId?: string | null
  nickname?: string
  credentials: {
    accessToken?: string
    refreshToken?: string
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
    region?: string
    authRegion?: string
    auth_region?: string
    apiRegion?: string
    api_region?: string
    authMethod?: string
    startUrl?: string
    provider?: string
  }
  idp?: string
  profileArn?: string
  profile_arn?: string
  machineId?: string
  groupId?: string
  group_id?: string
  groupName?: string
  group_name?: string
  subscription?: {
    type?: string
    title?: string
    overageCapability?: string
  }
  usage?: {
    current?: number
    limit?: number
    nextResetDate?: string | number
  }
  labels?: string[]
  tags?: string[]
  accountSource?: string
  account_source?: string
  accountSourceLabel?: string
  account_source_label?: string
  status?: string
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'duplicate' | 'failed' | 'skipped'
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

function normalizeKamLabels(account: KamAccount): string[] {
  const labels = [...(account.labels || []), ...(account.tags || [])]
  return Array.from(new Set(labels.map(label => label.trim()).filter(Boolean)))
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

function timestampSeconds(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value > 1_000_000_000_000 ? value / 1000 : value
  }
  if (typeof value === 'string' && value.trim()) {
    const millis = Date.parse(value)
    if (Number.isFinite(millis)) return millis / 1000
  }
  return undefined
}

function overageCapable(value: string | undefined): boolean | undefined {
  if (!value) return undefined
  const normalized = value.trim().toUpperCase()
  if (normalized === 'OVERAGE_CAPABLE') return true
  if (normalized === 'NOT_OVERAGE_CAPABLE' || normalized === 'NOT_AVAILABLE') return false
  return undefined
}

function importBalancePayload(account: KamAccount) {
  const currentUsage = finiteNumber(account.usage?.current)
  const usageLimit = finiteNumber(account.usage?.limit)
  const overageCapabilityRaw = firstNonEmptyString(account.subscription?.overageCapability)
  return {
    subscriptionTitle: firstNonEmptyString(account.subscription?.title, account.subscription?.type),
    currentUsage,
    usageLimit,
    nextResetAt: timestampSeconds(account.usage?.nextResetDate),
    overageCapable: overageCapable(overageCapabilityRaw),
    overageCapabilityRaw,
  }
}

function importedUsageText(account: KamAccount): string {
  const currentUsage = finiteNumber(account.usage?.current)
  const usageLimit = finiteNumber(account.usage?.limit)
  if (currentUsage !== undefined && usageLimit !== undefined) {
    return `${currentUsage}/${usageLimit}（导入快照）`
  }
  if (usageLimit !== undefined) {
    return `0/${usageLimit}（导入快照）`
  }
  return '已导入，余额稍后同步'
}

// 校验元素是否为有效的 KAM 账号结构
function isValidKamAccount(item: unknown): item is KamAccount {
  if (typeof item !== 'object' || item === null) return false
  const obj = item as Record<string, unknown>
  if (typeof obj.credentials !== 'object' || obj.credentials === null) return false
  const cred = obj.credentials as Record<string, unknown>
  const hasRefresh = typeof cred.refreshToken === 'string' && cred.refreshToken.trim().length > 0
  const accessToken = typeof cred.accessToken === 'string' ? cred.accessToken.trim() : ''
  const authMethod = typeof cred.authMethod === 'string' ? cred.authMethod.toLowerCase() : ''
  const hasApiKey =
    accessToken.startsWith('ksk_') ||
    authMethod === 'api_key' ||
    authMethod === 'apikey' ||
    authMethod === 'api-key'
  return hasRefresh || hasApiKey
}

// 将扁平结构（顶层 refreshToken）适配为 KAM 的 credentials 嵌套结构
function normalizeToKamAccount(item: unknown): unknown {
  if (typeof item !== 'object' || item === null) return item
  const obj = item as Record<string, unknown>
  // 已有 credentials 结构，无需转换
  if (typeof obj.credentials === 'object' && obj.credentials !== null) return item
  // 顶层有 refreshToken 或 api key accessToken，自动包装
  const topAccess = typeof obj.accessToken === 'string' ? obj.accessToken.trim() : ''
  const topAuth = typeof obj.authMethod === 'string' ? obj.authMethod.toLowerCase() : ''
  const topIsApiKey =
    topAccess.startsWith('ksk_') ||
    topAuth === 'api_key' ||
    topAuth === 'apikey' ||
    topAuth === 'api-key'
  if (
    (typeof obj.refreshToken === 'string' && obj.refreshToken.trim().length > 0) ||
    topIsApiKey
  ) {
    const {
      refreshToken,
      accessToken,
      clientId,
      clientSecret,
      tokenEndpoint,
      token_endpoint,
      issuerUrl,
      issuer_url,
      scopes,
      scope,
      profileArn,
      profile_arn,
      expiresAt,
      expires_at,
      region,
      authRegion,
      auth_region,
      apiRegion,
      api_region,
      authMethod,
      startUrl,
      provider,
      ...rest
    } = obj
    return {
      ...rest,
      credentials: {
        refreshToken,
        accessToken,
        clientId,
        clientSecret,
        tokenEndpoint,
        token_endpoint,
        issuerUrl,
        issuer_url,
        scopes,
        scope,
        profileArn,
        profile_arn,
        expiresAt,
        expires_at,
        region,
        authRegion,
        auth_region,
        apiRegion,
        api_region,
        authMethod,
        startUrl,
        provider,
      },
    }
  }
  return item
}

// 解析 KAM 导出 JSON，支持单账号和多账号格式，兼容扁平 refreshToken 结构
function parseKamJson(raw: string): KamAccount[] {
  const parsed = JSON.parse(raw)

  let rawItems: unknown[]

  // 标准 KAM 导出格式：{ version, accounts: [...] }
  if (parsed.accounts && Array.isArray(parsed.accounts)) {
    rawItems = parsed.accounts
  }
  // 兜底：如果直接是账号数组
  else if (Array.isArray(parsed)) {
    rawItems = parsed
  }
  // 单个账号对象（有 credentials 字段）
  else if (parsed.credentials && typeof parsed.credentials === 'object') {
    rawItems = [parsed]
  }
  // 单个扁平对象（顶层有 refreshToken / api key）
  else if (
    typeof parsed.refreshToken === 'string' ||
    (typeof parsed.accessToken === 'string' && String(parsed.accessToken).startsWith('ksk_'))
  ) {
    rawItems = [parsed]
  }
  else {
    throw new Error('无法识别的 KAM JSON 格式')
  }

  // 适配扁平结构为 KAM 嵌套结构
  rawItems = rawItems.map(normalizeToKamAccount)

  const validAccounts = rawItems.filter(isValidKamAccount)

  if (rawItems.length > 0 && validAccounts.length === 0) {
    throw new Error(`共 ${rawItems.length} 条记录，但均缺少有效的 credentials.refreshToken / API Key`)
  }

  if (validAccounts.length < rawItems.length) {
    const skipped = rawItems.length - validAccounts.length
    console.warn(`KAM 导入：跳过 ${skipped} 条缺少有效 credentials.refreshToken / API Key 的记录`)
  }

  return validAccounts
}

export function KamImportDialog({ open, onOpenChange }: KamImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [skipErrorAccounts, setSkipErrorAccounts] = useState(true)
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
      return { success: false, error: `禁用失败: ${extractErrorMessage(error)}` }
    }
    try {
      await deleteCredential(id)
      return { success: true }
    } catch (error) {
      return { success: false, error: `删除失败: ${extractErrorMessage(error)}` }
    }
  }

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  const handleImport = async () => {
    try {
      const accounts = parseKamJson(jsonInput)

      if (accounts.length === 0) {
        toast.error('没有可导入的账号')
        return
      }

      // 过滤无效账号（refreshToken 或 API Key）
      const validAccounts = accounts.filter(a => {
        const rt = a.credentials?.refreshToken?.trim()
        const at = a.credentials?.accessToken?.trim() || ''
        const am = a.credentials?.authMethod?.toLowerCase() || ''
        return !!rt || at.startsWith('ksk_') || am === 'api_key' || am === 'apikey' || am === 'api-key'
      })
      if (validAccounts.length === 0) {
        toast.error('没有包含有效 refreshToken / API Key 的账号')
        return
      }

      setImporting(true)
      setProgress({ current: 0, total: validAccounts.length })

      // 初始化结果，标记 error 状态的账号
      const initialResults: VerificationResult[] = validAccounts.map((account, i) => {
        if (skipErrorAccounts && account.status === 'error') {
          return { index: i + 1, status: 'skipped' as const, email: account.email || account.nickname }
        }
        return { index: i + 1, status: 'pending' as const, email: account.email || account.nickname }
      })
      setResults(initialResults)

      // 重复检测
      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      let successCount = 0
      let duplicateCount = 0
      let failCount = 0
      let skippedCount = 0

      for (let i = 0; i < validAccounts.length; i++) {
        const account = validAccounts[i]

        // 跳过 error 状态的账号
        if (skipErrorAccounts && account.status === 'error') {
          skippedCount++
          setProgress({ current: i + 1, total: validAccounts.length })
          continue
        }

        const cred = account.credentials
        const accessTokenValue = cred.accessToken?.trim() || ''
        const refreshTokenValue = cred.refreshToken?.trim() || ''
        const rawAuthMethod = cred.authMethod?.trim()
        const isApiKey =
          textIncludesAny(rawAuthMethod, ['api_key', 'apikey', 'api-key']) ||
          (!!accessTokenValue && accessTokenValue.startsWith('ksk_') && !refreshTokenValue)
        const token = isApiKey ? accessTokenValue : refreshTokenValue
        const tokenHash = token ? await sha256Hex(token) : null

        setCurrentProcessing(`正在处理 ${account.email || account.nickname || `账号 ${i + 1}`}`)
        setResults(prev => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'checking' }
          return next
        })

        if (!token) {
          failCount++
          setResults(prev => {
            const next = [...prev]
            next[i] = {
              ...next[i],
              status: 'failed',
              error: isApiKey ? '缺少 API Key (accessToken)' : '缺少 refreshToken',
              email: account.email || account.nickname,
            }
            return next
          })
          setProgress({ current: i + 1, total: validAccounts.length })
          continue
        }

        // 检查重复
        if (tokenHash && existingTokenHashes.has(tokenHash)) {
          duplicateCount++
          const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === tokenHash)
          setResults(prev => {
            const next = [...prev]
            next[i] = { ...next[i], status: 'duplicate', error: '该凭据已存在', email: existingCred?.email || account.email }
            return next
          })
          setProgress({ current: i + 1, total: validAccounts.length })
          continue
        }

        // 验活中
        setResults(prev => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'verifying' }
          return next
        })

        let addedCredId: number | null = null

        try {
          if (isApiKey) {
            if (!token.startsWith('ksk_') || token.length < 20) {
              throw new Error('API Key 格式无效，应以 ksk_ 开头且至少 20 字符')
            }
            const addedCred = await addCredential({
              accessToken: token,
              authMethod: 'api_key',
              accountSource: firstNonEmptyString(account.accountSource, account.account_source) || 'api_key',
              accountSourceLabel: firstNonEmptyString(account.accountSourceLabel, account.account_source_label) || 'API Key',
              machineId: account.machineId?.trim() || undefined,
              email: firstNonEmptyString(account.email, account.nickname),
              kamIdp: account.idp?.trim() || undefined,
              kamProvider: (cred.provider || account.idp)?.trim() || undefined,
              kamGroupId: firstNonEmptyString(account.groupId, account.group_id),
              kamGroupName: firstNonEmptyString(account.groupName, account.group_name),
              labels: normalizeKamLabels(account),
            })
            addedCredId = addedCred.credentialId
            successCount++
            if (tokenHash) existingTokenHashes.add(tokenHash)
            setResults(prev => {
              const next = [...prev]
              next[i] = {
                ...next[i],
                status: 'verified',
                usage: 'API Key（无需余额验活）',
                email: addedCred.email || account.email || account.nickname,
                credentialId: addedCred.credentialId,
              }
              return next
            })
            setProgress({ current: i + 1, total: validAccounts.length })
            continue
          }

          const clientId = cred.clientId?.trim() || undefined
          const clientSecret = cred.clientSecret?.trim() || undefined
          const explicitTokenEndpoint = firstNonEmptyString(cred.tokenEndpoint, cred.token_endpoint)
          const explicitIssuerUrl = firstNonEmptyString(cred.issuerUrl, cred.issuer_url)
          const inferredIssuerUrl = extractMicrosoftIssuerUrl(explicitIssuerUrl, account.userId)
          const explicitScopes = firstNonEmptyString(cred.scopes, cred.scope)
          const provider = cred.provider || account.idp
          const exportedRegion = cred.region?.trim() || undefined
          const exportedAuthRegion = firstNonEmptyString(cred.authRegion, cred.auth_region, exportedRegion)
          const exportedApiRegion = firstNonEmptyString(cred.apiRegion, cred.api_region, exportedRegion)
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
              userId: account.userId,
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
            authRegion: exportedAuthRegion,
            apiRegion: exportedApiRegion,
            clientId,
            clientSecret: authMethod === 'idc' ? clientSecret : undefined,
            tokenEndpoint: authMethod === 'external_idp' ? externalIdpMetadata.tokenEndpoint : undefined,
            issuerUrl: authMethod === 'external_idp' ? externalIdpMetadata.issuerUrl : undefined,
            scopes: authMethod === 'external_idp' ? externalIdpMetadata.scopes : undefined,
            profileArn: firstNonEmptyString(cred.profileArn, cred.profile_arn, account.profileArn, account.profile_arn),
            expiresAt: authMethod === 'external_idp'
              ? normalizeExpiresAt(firstNonEmptyString(cred.expiresAt, cred.expires_at) ?? cred.expiresAt ?? cred.expires_at)
              : undefined,
            machineId: account.machineId?.trim() || undefined,
            email: firstNonEmptyString(account.email, account.nickname),
            accountSource: firstNonEmptyString(account.accountSource, account.account_source) || 'kam',
            accountSourceLabel: firstNonEmptyString(account.accountSourceLabel, account.account_source_label) || 'KAM',
            kamIdp: account.idp?.trim() || undefined,
            kamProvider: provider?.trim() || undefined,
            kamGroupId: firstNonEmptyString(account.groupId, account.group_id),
            kamGroupName: firstNonEmptyString(account.groupName, account.group_name),
            labels: normalizeKamLabels(account),
            ...importBalancePayload(account),
          })

          addedCredId = addedCred.credentialId

          await new Promise(resolve => setTimeout(resolve, 1000))

          let balance
          try {
            balance = await getCredentialBalance(addedCred.credentialId)
          } catch (balanceError) {
            if (!isBalanceSoftError(balanceError)) {
              throw balanceError
            }

            successCount++
            if (tokenHash) existingTokenHashes.add(tokenHash)
            setCurrentProcessing(`导入成功: ${addedCred.email || account.email || `账号 ${i + 1}`}`)
            setResults(prev => {
              const next = [...prev]
              next[i] = {
                ...next[i],
                status: 'verified',
                usage: importedUsageText(account),
                email: addedCred.email || account.email,
                credentialId: addedCred.credentialId,
              }
              return next
            })
            setProgress({ current: i + 1, total: validAccounts.length })
            continue
          }

          successCount++
          if (tokenHash) existingTokenHashes.add(tokenHash)
          setCurrentProcessing(`验活成功: ${addedCred.email || account.email || `账号 ${i + 1}`}`)
          setResults(prev => {
            const next = [...prev]
            next[i] = {
              ...next[i],
              status: 'verified',
              usage: `${balance.currentUsage}/${balance.usageLimit}`,
              email: addedCred.email || account.email,
              credentialId: addedCred.credentialId,
            }
            return next
          })
        } catch (error) {
          let rollbackStatus: VerificationResult['rollbackStatus'] = 'skipped'
          let rollbackError: string | undefined

          if (addedCredId) {
            const result = await rollbackCredential(addedCredId)
            if (result.success) {
              rollbackStatus = 'success'
            } else {
              rollbackStatus = 'failed'
              rollbackError = result.error
            }
          }

          failCount++
          setResults(prev => {
            const next = [...prev]
            next[i] = {
              ...next[i],
              status: 'failed',
              error: extractErrorMessage(error),
              rollbackStatus,
              rollbackError,
            }
            return next
          })
        }

        setProgress({ current: i + 1, total: validAccounts.length })
      }

      // 汇总
      const parts: string[] = []
      if (successCount > 0) parts.push(`成功 ${successCount}`)
      if (duplicateCount > 0) parts.push(`重复 ${duplicateCount}`)
      if (failCount > 0) parts.push(`失败 ${failCount}`)
      if (skippedCount > 0) parts.push(`跳过 ${skippedCount}`)

      if (failCount === 0 && duplicateCount === 0 && skippedCount === 0) {
        toast.success(`成功导入并验活 ${successCount} 个凭据`)
      } else {
        toast.info(`导入完成：${parts.join('，')}`)
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
      case 'skipped':
        return <AlertCircle className="w-5 h-5 text-gray-400" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-nb-red" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending': return '等待中'
      case 'checking': return '检查重复...'
      case 'verifying': return '验活中...'
      case 'verified': return '验活成功'
      case 'duplicate': return '重复凭据'
      case 'skipped': return '已跳过（error 状态）'
      case 'failed':
        if (result.rollbackStatus === 'success') return '验活失败（已排除）'
        if (result.rollbackStatus === 'failed') return '验活失败（未排除）'
        return '验活失败（未创建）'
    }
  }

  // 预览解析结果
  const { previewAccounts, parseError } = useMemo(() => {
    if (!jsonInput.trim()) return { previewAccounts: [] as KamAccount[], parseError: '' }
    try {
      return { previewAccounts: parseKamJson(jsonInput), parseError: '' }
    } catch (e) {
      return { previewAccounts: [] as KamAccount[], parseError: extractErrorMessage(e) }
    }
  }, [jsonInput])

  const errorAccountCount = previewAccounts.filter(a => a.status === 'error').length

  return (
    <Dialog
      open={open}
      onOpenChange={(newOpen) => {
        if (!newOpen && importing) return
        if (!newOpen) resetForm()
        onOpenChange(newOpen)
      }}
    >
      <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>KAM 账号导入（自动验活）</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">KAM 导出 JSON</label>
            <textarea
              placeholder={'粘贴 KAM 导出 JSON 或将文件拖拽到此处'}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              onDragOver={(e) => { e.preventDefault(); e.stopPropagation() }}
              onDrop={(e) => {
                e.preventDefault()
                e.stopPropagation()
                const file = e.dataTransfer.files[0]
                if (file) {
                  const reader = new FileReader()
                  reader.onload = (ev) => {
                    const text = ev.target?.result
                    if (typeof text === 'string') setJsonInput(text)
                  }
                  reader.readAsText(file)
                }
              }}
              disabled={importing}
              className="flex min-h-[200px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 font-mono"
            />
            <p className="text-xs text-muted-foreground">支持粘贴 JSON 文本或直接拖入 .json 文件</p>
          </div>

          {/* 解析预览 */}
          {parseError && (
            <div className="text-sm text-nb-red">解析失败: {parseError}</div>
          )}
          {previewAccounts.length > 0 && !importing && results.length === 0 && (
            <div className="space-y-2">
              <div className="text-sm text-muted-foreground">
                识别到 {previewAccounts.length} 个账号
                {errorAccountCount > 0 && `（其中 ${errorAccountCount} 个为 error 状态）`}
              </div>
              {errorAccountCount > 0 && (
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={skipErrorAccounts}
                    onChange={(e) => setSkipErrorAccounts(e.target.checked)}
                    className="rounded border-gray-300"
                  />
                  跳过 error 状态的账号
                </label>
              )}
            </div>
          )}

          {/* 导入进度和结果 */}
          {(importing || results.length > 0) && (
            <>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? '导入进度' : '导入完成'}</span>
                  <span>{progress.current} / {progress.total}</span>
                </div>
                <div className="w-full bg-muted border border-border rounded-sm h-2">
                  <div
                    className="bg-primary h-2 rounded-sm transition-all"
                    style={{ width: `${progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">{currentProcessing}</div>
                )}
              </div>

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
                <span className="text-gray-500">
                  ○ 跳过: {results.filter(r => r.status === 'skipped').length}
                </span>
              </div>

              <div className="border-[2.5px] border-border rounded-sm divide-y divide-border max-h-[300px] overflow-y-auto">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || `账号 #${result.index}`}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">用量: {result.usage}</div>
                        )}
                        {result.error && (
                          <div className="text-xs text-nb-red mt-1">{result.error}</div>
                        )}
                        {result.rollbackError && (
                          <div className="text-xs text-nb-red mt-1">回滚失败: {result.rollbackError}</div>
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
            onClick={() => { onOpenChange(false); resetForm() }}
            disabled={importing}
          >
            {importing ? '导入中...' : results.length > 0 ? '关闭' : '取消'}
          </Button>
          {results.length === 0 && (
            <Button
              type="button"
              onClick={handleImport}
              disabled={importing || !jsonInput.trim() || previewAccounts.length === 0 || !!parseError}
            >
              开始导入并验活
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
