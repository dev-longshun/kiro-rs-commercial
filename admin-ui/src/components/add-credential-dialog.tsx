import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
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
import { useAddCredential } from '@/hooks/use-credentials'
import {
  cancelKiroSsoLogin,
  completeIamSsoLogin,
  importSsoToken,
  pollBuilderIdLogin,
  pollKiroSsoLogin,
  startBuilderIdLogin,
  startIamSsoLogin,
  startKiroSsoLogin,
} from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { BuilderIdStartResponse, IamSsoStartResponse, KiroSsoStartResponse } from '@/types/api'

interface AddCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type AuthMethod = 'social' | 'idc' | 'external_idp' | 'api_key'
type EntryMode = 'manual' | 'builder_id' | 'iam_sso' | 'kiro_sso' | 'sso_token'

export function AddCredentialDialog({ open, onOpenChange }: AddCredentialDialogProps) {
  const [entryMode, setEntryMode] = useState<EntryMode>('manual')
  const [refreshToken, setRefreshToken] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [authMethod, setAuthMethod] = useState<AuthMethod>('social')
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [tokenEndpoint, setTokenEndpoint] = useState('')
  const [issuerUrl, setIssuerUrl] = useState('')
  const [scopes, setScopes] = useState('')
  const [priority, setPriority] = useState('0')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [authFlowRegion, setAuthFlowRegion] = useState('')
  const [flowPending, setFlowPending] = useState(false)
  const [builderSession, setBuilderSession] = useState<BuilderIdStartResponse | null>(null)
  const [iamStartUrl, setIamStartUrl] = useState('')
  const [iamSession, setIamSession] = useState<IamSsoStartResponse | null>(null)
  const [iamCallbackUrl, setIamCallbackUrl] = useState('')
  const [kiroSession, setKiroSession] = useState<KiroSsoStartResponse | null>(null)
  const [ssoBearerToken, setSsoBearerToken] = useState('')

  const { mutate, isPending } = useAddCredential()
  const queryClient = useQueryClient()

  const resetForm = () => {
    setEntryMode('manual')
    setRefreshToken('')
    setApiKey('')
    setAuthMethod('social')
    setAuthRegion('')
    setApiRegion('')
    setClientId('')
    setClientSecret('')
    setTokenEndpoint('')
    setIssuerUrl('')
    setScopes('')
    setPriority('0')
    setMachineId('')
    setProxyUrl('')
    setProxyUsername('')
    setProxyPassword('')
    setAuthFlowRegion('')
    setFlowPending(false)
    setBuilderSession(null)
    setIamStartUrl('')
    setIamSession(null)
    setIamCallbackUrl('')
    setKiroSession(null)
    setSsoBearerToken('')
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    if (entryMode !== 'manual') {
      return
    }

    if (authMethod === 'api_key') {
      const key = apiKey.trim()
      if (!key) {
        toast.error('请输入 API Key')
        return
      }
      if (!key.startsWith('ksk_') || key.length < 20) {
        toast.error('API Key 格式无效，应以 ksk_ 开头且至少 20 字符')
        return
      }

      mutate(
        {
          accessToken: key,
          authMethod: 'api_key',
          accountSource: 'api_key',
          accountSourceLabel: 'API Key',
          priority: parseInt(priority) || 0,
          machineId: machineId.trim() || undefined,
          proxyUrl: proxyUrl.trim() || undefined,
          proxyUsername: proxyUsername.trim() || undefined,
          proxyPassword: proxyPassword.trim() || undefined,
        },
        {
          onSuccess: (data) => {
            toast.success(data.message)
            onOpenChange(false)
            resetForm()
          },
          onError: (error: unknown) => {
            toast.error(`添加失败: ${extractErrorMessage(error)}`)
          },
        }
      )
      return
    }

    // 验证必填字段
    if (!refreshToken.trim()) {
      toast.error('请输入 Refresh Token')
      return
    }

    // IdC/Builder-ID/IAM 需要额外字段
    if (authMethod === 'idc' && (!clientId.trim() || !clientSecret.trim())) {
      toast.error('IdC/Builder-ID/IAM 认证需要填写 Client ID 和 Client Secret')
      return
    }

    if (authMethod === 'external_idp' && (!clientId.trim() || !tokenEndpoint.trim())) {
      toast.error('External IdP 认证需要填写 Client ID 和 Token Endpoint')
      return
    }

    mutate(
      {
        refreshToken: refreshToken.trim(),
        authMethod,
        authRegion: authRegion.trim() || undefined,
        apiRegion: apiRegion.trim() || undefined,
        clientId: clientId.trim() || undefined,
        clientSecret: authMethod === 'idc' ? clientSecret.trim() || undefined : undefined,
        tokenEndpoint: authMethod === 'external_idp' ? tokenEndpoint.trim() || undefined : undefined,
        issuerUrl: authMethod === 'external_idp' ? issuerUrl.trim() || undefined : undefined,
        scopes: authMethod === 'external_idp' ? scopes.trim() || undefined : undefined,
        priority: parseInt(priority) || 0,
        machineId: machineId.trim() || undefined,
        proxyUrl: proxyUrl.trim() || undefined,
        proxyUsername: proxyUsername.trim() || undefined,
        proxyPassword: proxyPassword.trim() || undefined,
      },
      {
        onSuccess: (data) => {
          toast.success(data.message)
          onOpenChange(false)
          resetForm()
        },
        onError: (error: unknown) => {
          toast.error(`添加失败: ${extractErrorMessage(error)}`)
        },
      }
    )
  }

  const finishAuthFlow = (message: string) => {
    queryClient.invalidateQueries({ queryKey: ['credentials'] })
    toast.success(message)
    onOpenChange(false)
    resetForm()
  }

  const handleStartBuilderId = async () => {
    setFlowPending(true)
    try {
      const session = await startBuilderIdLogin(authFlowRegion.trim() || undefined)
      setBuilderSession(session)
      toast.success('Builder ID 登录已启动')
    } catch (error) {
      toast.error(`启动失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handlePollBuilderId = async () => {
    if (!builderSession) return
    setFlowPending(true)
    try {
      const result = await pollBuilderIdLogin(builderSession.sessionId)
      if (result.completed && result.account) {
        finishAuthFlow(result.account.message)
      } else {
        toast.info(result.status || '等待授权完成')
      }
    } catch (error) {
      toast.error(`轮询失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handleStartIamSso = async () => {
    if (!iamStartUrl.trim()) {
      toast.error('请输入 Start URL')
      return
    }
    setFlowPending(true)
    try {
      const session = await startIamSsoLogin(iamStartUrl.trim(), authFlowRegion.trim() || undefined)
      setIamSession(session)
      toast.success('IAM SSO 登录已启动')
    } catch (error) {
      toast.error(`启动失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handleCompleteIamSso = async () => {
    if (!iamSession || !iamCallbackUrl.trim()) {
      toast.error('请输入回调 URL')
      return
    }
    setFlowPending(true)
    try {
      const result = await completeIamSsoLogin(iamSession.sessionId, iamCallbackUrl.trim())
      finishAuthFlow(result.message)
    } catch (error) {
      toast.error(`完成失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handleStartKiroSso = async () => {
    setFlowPending(true)
    try {
      const session = await startKiroSsoLogin(authFlowRegion.trim() || undefined)
      setKiroSession(session)
      toast.success('Kiro SSO 登录已启动')
    } catch (error) {
      toast.error(`启动失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handlePollKiroSso = async () => {
    if (!kiroSession) return
    setFlowPending(true)
    try {
      const result = await pollKiroSsoLogin(kiroSession.sessionId)
      if (result.completed && result.account) {
        finishAuthFlow(result.account.message)
      } else {
        toast.info(result.status || '等待授权完成')
      }
    } catch (error) {
      toast.error(`轮询失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handleCancelKiroSso = async () => {
    if (!kiroSession) return
    setFlowPending(true)
    try {
      const result = await cancelKiroSsoLogin(kiroSession.sessionId)
      toast.success(result.message)
      setKiroSession(null)
    } catch (error) {
      toast.error(`取消失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  const handleImportSsoToken = async () => {
    if (!ssoBearerToken.trim()) {
      toast.error('请输入 SSO Token')
      return
    }
    setFlowPending(true)
    try {
      const result = await importSsoToken(ssoBearerToken.trim(), authFlowRegion.trim() || undefined)
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
      if (result.errors.length > 0) {
        toast.warning(`导入完成：成功 ${result.accounts.length} 个，失败 ${result.errors.length} 个`)
      } else {
        toast.success(`成功导入 ${result.accounts.length} 个账号`)
      }
      onOpenChange(false)
      resetForm()
    } catch (error) {
      toast.error(`导入失败: ${extractErrorMessage(error)}`)
    } finally {
      setFlowPending(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>添加凭据</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            <div className="space-y-2">
              <label htmlFor="entryMode" className="text-sm font-medium">
                添加方式
              </label>
              <select
                id="entryMode"
                value={entryMode}
                onChange={(e) => setEntryMode(e.target.value as EntryMode)}
                disabled={isPending || flowPending}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <option value="manual">手动 Refresh Token</option>
                <option value="builder_id">Builder ID Device Code</option>
                <option value="iam_sso">IAM Identity Center SSO</option>
                <option value="kiro_sso">Kiro SSO</option>
                <option value="sso_token">SSO Token 导入</option>
              </select>
            </div>

            {entryMode !== 'manual' && (
              <div className="space-y-2">
                <label className="text-sm font-medium">Region</label>
                <Input
                  placeholder="留空使用默认 Region"
                  value={authFlowRegion}
                  onChange={(e) => setAuthFlowRegion(e.target.value)}
                  disabled={flowPending}
                />
              </div>
            )}

            {entryMode === 'manual' && (
              <>
            {/* 认证方式 */}
            <div className="space-y-2">
              <label htmlFor="authMethod" className="text-sm font-medium">
                认证方式
              </label>
              <select
                id="authMethod"
                value={authMethod}
                onChange={(e) => setAuthMethod(e.target.value as AuthMethod)}
                disabled={isPending}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <option value="social">Social</option>
                <option value="idc">IdC/Builder-ID/IAM</option>
                <option value="external_idp">External IdP / Microsoft 365</option>
                <option value="api_key">API Key</option>
              </select>
            </div>

            {authMethod === 'api_key' ? (
              <div className="space-y-2">
                <label htmlFor="apiKey" className="text-sm font-medium">
                  API Key <span className="text-nb-red">*</span>
                </label>
                <Input
                  id="apiKey"
                  type="password"
                  placeholder="ksk_..."
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  disabled={isPending}
                />
                <p className="text-xs text-muted-foreground">
                  静态密钥，以 ksk_ 开头，无需 OAuth 刷新
                </p>
              </div>
            ) : (
              <div className="space-y-2">
                <label htmlFor="refreshToken" className="text-sm font-medium">
                  Refresh Token <span className="text-nb-red">*</span>
                </label>
                <Input
                  id="refreshToken"
                  type="password"
                  placeholder="请输入 Refresh Token"
                  value={refreshToken}
                  onChange={(e) => setRefreshToken(e.target.value)}
                  disabled={isPending}
                />
              </div>
            )}

            {/* Region 配置（API Key 无需刷新，可跳过） */}
            {authMethod !== 'api_key' && (
            <div className="space-y-2">
              <label className="text-sm font-medium">Region 配置</label>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <Input
                    id="authRegion"
                    placeholder="Auth Region"
                    value={authRegion}
                    onChange={(e) => setAuthRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div>
                  <Input
                    id="apiRegion"
                    placeholder="API Region"
                    value={apiRegion}
                    onChange={(e) => setApiRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                均可留空使用全局配置。Auth Region 用于 Token 刷新，API Region 用于 API 请求
              </p>
            </div>
            )}

            {/* IdC/Builder-ID/IAM 额外字段 */}
            {authMethod === 'idc' && (
              <>
                <div className="space-y-2">
                  <label htmlFor="clientId" className="text-sm font-medium">
                    Client ID <span className="text-nb-red">*</span>
                  </label>
                  <Input
                    id="clientId"
                    placeholder="请输入 Client ID"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="clientSecret" className="text-sm font-medium">
                    Client Secret <span className="text-nb-red">*</span>
                  </label>
                  <Input
                    id="clientSecret"
                    type="password"
                    placeholder="请输入 Client Secret"
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {authMethod === 'external_idp' && (
              <>
                <div className="space-y-2">
                  <label htmlFor="externalClientId" className="text-sm font-medium">
                    Client ID <span className="text-nb-red">*</span>
                  </label>
                  <Input
                    id="externalClientId"
                    placeholder="请输入 Public Client ID"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="tokenEndpoint" className="text-sm font-medium">
                    Token Endpoint <span className="text-nb-red">*</span>
                  </label>
                  <Input
                    id="tokenEndpoint"
                    placeholder="https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"
                    value={tokenEndpoint}
                    onChange={(e) => setTokenEndpoint(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="issuerUrl" className="text-sm font-medium">
                    Issuer URL
                  </label>
                  <Input
                    id="issuerUrl"
                    placeholder="https://login.microsoftonline.com/{tenant}/v2.0"
                    value={issuerUrl}
                    onChange={(e) => setIssuerUrl(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="scopes" className="text-sm font-medium">
                    Scopes
                  </label>
                  <Input
                    id="scopes"
                    placeholder="api://.../codewhisperer:conversations offline_access"
                    value={scopes}
                    onChange={(e) => setScopes(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* 优先级 */}
            <div className="space-y-2">
              <label htmlFor="priority" className="text-sm font-medium">
                优先级
              </label>
              <Input
                id="priority"
                type="number"
                min="0"
                placeholder="数字越小优先级越高"
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
                disabled={isPending}
              />
              <p className="text-xs text-muted-foreground">
                数字越小优先级越高，默认为 0
              </p>
            </div>

            {/* Machine ID */}
            <div className="space-y-2">
              <label htmlFor="machineId" className="text-sm font-medium">
                Machine ID
              </label>
              <Input
                id="machineId"
                placeholder="留空使用配置中字段, 否则由刷新Token自动派生"
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
              <p className="text-xs text-muted-foreground">
                可选，64 位十六进制字符串，留空使用配置中字段, 否则由刷新Token自动派生
              </p>
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">代理配置</label>
              <Input
                id="proxyUrl"
                placeholder='代理 URL（留空使用全局配置，"direct" 不使用代理）'
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  id="proxyUsername"
                  placeholder="代理用户名"
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  id="proxyPassword"
                  type="password"
                  placeholder="代理密码"
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                留空使用全局代理。输入 "direct" 可显式不使用代理
              </p>
            </div>
              </>
            )}

            {entryMode === 'builder_id' && (
              <div className="space-y-4 border-[2.5px] border-border rounded-sm p-3">
                <div className="flex gap-2">
                  <Button type="button" onClick={handleStartBuilderId} disabled={flowPending}>
                    启动登录
                  </Button>
                  <Button type="button" variant="outline" onClick={handlePollBuilderId} disabled={flowPending || !builderSession}>
                    检查授权
                  </Button>
                </div>
                {builderSession && (
                  <div className="space-y-2 text-sm">
                    <div>验证码：<span className="font-mono font-bold">{builderSession.userCode}</span></div>
                    <a href={builderSession.verificationUri} target="_blank" rel="noreferrer" className="text-primary underline">
                      {builderSession.verificationUri}
                    </a>
                  </div>
                )}
              </div>
            )}

            {entryMode === 'iam_sso' && (
              <div className="space-y-4 border-[2.5px] border-border rounded-sm p-3">
                <Input
                  placeholder="Start URL"
                  value={iamStartUrl}
                  onChange={(e) => setIamStartUrl(e.target.value)}
                  disabled={flowPending}
                />
                <div className="flex gap-2">
                  <Button type="button" onClick={handleStartIamSso} disabled={flowPending}>
                    启动登录
                  </Button>
                </div>
                {iamSession && (
                  <div className="space-y-3">
                    <a href={iamSession.authorizeUrl} target="_blank" rel="noreferrer" className="break-all text-sm text-primary underline">
                      {iamSession.authorizeUrl}
                    </a>
                    <Input
                      placeholder="粘贴授权后的完整回调 URL"
                      value={iamCallbackUrl}
                      onChange={(e) => setIamCallbackUrl(e.target.value)}
                      disabled={flowPending}
                    />
                    <Button type="button" onClick={handleCompleteIamSso} disabled={flowPending}>
                      完成导入
                    </Button>
                  </div>
                )}
              </div>
            )}

            {entryMode === 'kiro_sso' && (
              <div className="space-y-4 border-[2.5px] border-border rounded-sm p-3">
                <div className="flex gap-2">
                  <Button type="button" onClick={handleStartKiroSso} disabled={flowPending}>
                    启动登录
                  </Button>
                  <Button type="button" variant="outline" onClick={handlePollKiroSso} disabled={flowPending || !kiroSession}>
                    检查授权
                  </Button>
                  {kiroSession && (
                    <Button type="button" variant="ghost" onClick={handleCancelKiroSso} disabled={flowPending}>
                      取消
                    </Button>
                  )}
                </div>
                {kiroSession && (
                  <a href={kiroSession.signInUrl} target="_blank" rel="noreferrer" className="break-all text-sm text-primary underline">
                    {kiroSession.signInUrl}
                  </a>
                )}
              </div>
            )}

            {entryMode === 'sso_token' && (
              <div className="space-y-4 border-[2.5px] border-border rounded-sm p-3">
                <textarea
                  className="min-h-28 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                  placeholder="粘贴 Bearer Token"
                  value={ssoBearerToken}
                  onChange={(e) => setSsoBearerToken(e.target.value)}
                  disabled={flowPending}
                />
                <Button type="button" onClick={handleImportSsoToken} disabled={flowPending}>
                  导入 Token
                </Button>
              </div>
            )}
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
            {entryMode === 'manual' && (
              <Button type="submit" disabled={isPending || (authMethod === 'api_key' ? !apiKey.trim() : !refreshToken.trim())}>
                {isPending ? '添加中...' : '添加'}
              </Button>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
