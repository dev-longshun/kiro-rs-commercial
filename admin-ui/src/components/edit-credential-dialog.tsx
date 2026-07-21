import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useUpdateCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialStatusItem } from '@/types/api'

interface EditCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

export function EditCredentialDialog({ open, onOpenChange, credential }: EditCredentialDialogProps) {
  const [email, setEmail] = useState('')
  const [accessToken, setAccessToken] = useState('')
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [accountSource, setAccountSource] = useState('')
  const [accountSourceLabel, setAccountSourceLabel] = useState('')
  const [kamIdp, setKamIdp] = useState('')
  const [kamProvider, setKamProvider] = useState('')
  const [kamGroupId, setKamGroupId] = useState('')
  const [kamGroupName, setKamGroupName] = useState('')
  const [labels, setLabels] = useState('')

  const { mutate, isPending } = useUpdateCredential()
  const isApiKey = credential.authMethod === 'api_key'

  // 当对话框打开或凭据变化时，重置表单
  useEffect(() => {
    if (open) {
      setEmail(credential.email || '')
      setAccessToken('')
      setAuthRegion('')
      setApiRegion('')
      setClientId('')
      setClientSecret('')
      setMachineId('')
      setProxyUrl(credential.proxyUrl || '')
      setProxyUsername('')
      setProxyPassword('')
      setAccountSource(credential.accountSource || '')
      setAccountSourceLabel(credential.accountSourceLabel || '')
      setKamIdp(credential.kamIdp || '')
      setKamProvider(credential.kamProvider || '')
      setKamGroupId(credential.kamGroupId || '')
      setKamGroupName(credential.kamGroupName || '')
      setLabels((credential.labels || []).join(', '))
    }
  }, [open, credential])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    // 构建只包含有变更的字段
    const data: Record<string, string | string[]> = {}
    if (email !== (credential.email || '')) data.email = email
    if (isApiKey && accessToken.trim()) {
      const key = accessToken.trim()
      if (!key.startsWith('ksk_') || key.length < 20) {
        toast.error('API Key 格式无效，应以 ksk_ 开头且至少 20 字符')
        return
      }
      data.accessToken = key
    }
    if (authRegion !== '') data.authRegion = authRegion
    if (apiRegion !== '') data.apiRegion = apiRegion
    if (clientId !== '') data.clientId = clientId
    if (clientSecret !== '') data.clientSecret = clientSecret
    if (machineId !== '') data.machineId = machineId
    if (proxyUrl !== (credential.proxyUrl || '')) data.proxyUrl = proxyUrl
    if (proxyUsername !== '') data.proxyUsername = proxyUsername
    if (proxyPassword !== '') data.proxyPassword = proxyPassword
    if (accountSource !== (credential.accountSource || '')) data.accountSource = accountSource
    if (accountSourceLabel !== (credential.accountSourceLabel || '')) data.accountSourceLabel = accountSourceLabel
    if (kamIdp !== (credential.kamIdp || '')) data.kamIdp = kamIdp
    if (kamProvider !== (credential.kamProvider || '')) data.kamProvider = kamProvider
    if (kamGroupId !== (credential.kamGroupId || '')) data.kamGroupId = kamGroupId
    if (kamGroupName !== (credential.kamGroupName || '')) data.kamGroupName = kamGroupName
    const nextLabels = labels.split(',').map(label => label.trim()).filter(Boolean)
    if (nextLabels.join('\n') !== (credential.labels || []).join('\n')) data.labels = nextLabels

    if (Object.keys(data).length === 0) {
      toast.info('没有需要更新的字段')
      return
    }

    mutate(
      { id: credential.id, data },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          onOpenChange(false)
        },
        onError: (error: unknown) => {
          toast.error(`更新失败: ${extractErrorMessage(error)}`)
        },
      }
    )
  }

  const isIdc = credential.authMethod === 'idc'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>编辑凭据 #{credential.id}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            <p className="text-xs text-muted-foreground">
              只填写需要修改的字段，留空的字段不会被更改。
            </p>

            {isApiKey && (
              <div className="space-y-2">
                <label className="text-sm font-medium">API Key</label>
                <Input
                  type="password"
                  placeholder="留空则不修改；填写新 ksk_... 可替换"
                  value={accessToken}
                  onChange={(e) => setAccessToken(e.target.value)}
                  disabled={isPending}
                />
              </div>
            )}
            <div className="space-y-2">
              <label className="text-sm font-medium">邮箱</label>
              <Input
                placeholder="账号邮箱"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* Region 配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Region 配置</label>
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="Auth Region"
                  value={authRegion}
                  onChange={(e) => setAuthRegion(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="API Region"
                  value={apiRegion}
                  onChange={(e) => setApiRegion(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                Auth Region 用于 Token 刷新，API Region 用于 API 请求
              </p>
            </div>

            {/* IdC 字段 */}
            {isIdc && (
              <>
                <div className="space-y-2">
                  <label className="text-sm font-medium">Client ID</label>
                  <Input
                    placeholder="留空不修改"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium">Client Secret</label>
                  <Input
                    type="password"
                    placeholder="留空不修改"
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* Machine ID */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Machine ID</label>
              <Input
                placeholder="留空不修改"
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium">来源与 KAM 元数据</label>
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="来源，如 kam"
                  value={accountSource}
                  onChange={(e) => setAccountSource(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="来源标签"
                  value={accountSourceLabel}
                  onChange={(e) => setAccountSourceLabel(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="KAM IDP"
                  value={kamIdp}
                  onChange={(e) => setKamIdp(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="Provider"
                  value={kamProvider}
                  onChange={(e) => setKamProvider(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="Group ID"
                  value={kamGroupId}
                  onChange={(e) => setKamGroupId(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="Group Name"
                  value={kamGroupName}
                  onChange={(e) => setKamGroupName(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <Input
                placeholder="标签，逗号分隔"
                value={labels}
                onChange={(e) => setLabels(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">代理配置</label>
              <Input
                placeholder='代理 URL（"direct" 不使用代理）'
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="代理用户名"
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  type="password"
                  placeholder="代理密码"
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isPending}
            >
              取消
            </Button>
            <Button type="submit" disabled={isPending}>
              {isPending ? '更新中...' : '保存'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
