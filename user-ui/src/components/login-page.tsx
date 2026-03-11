import { useState } from 'react'
import { KeyRound, Loader2 } from 'lucide-react'
import { storage } from '@/lib/storage'
import { login } from '@/api/user'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { toast } from 'sonner'
import type { LoginResponse } from '@/types/api'

interface LoginPageProps {
  onLogin: (data: LoginResponse) => void
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const [apiKey, setApiKey] = useState('')
  const [loading, setLoading] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const key = apiKey.trim()
    if (!key) return

    setLoading(true)
    try {
      const data = await login(key)
      storage.setApiKey(key)
      onLogin(data)
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { error?: string } } }
      const msg = axiosErr.response?.data?.error || '登录失败，请检查 API Key'
      toast.error(msg)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
            <KeyRound className="h-6 w-6 text-primary" />
          </div>
          <CardTitle className="text-2xl">Kiro 用量监控</CardTitle>
          <CardDescription>
            请输入您的 API Key 查看用量数据
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Input
                type="password"
                placeholder="sk-..."
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                className="text-center font-mono"
              />
            </div>
            <Button type="submit" className="w-full" disabled={!apiKey.trim() || loading}>
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              {loading ? '验证中...' : '查看用量'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
