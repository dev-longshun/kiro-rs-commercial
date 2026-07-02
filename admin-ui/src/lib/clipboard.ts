function copyWithTextarea(text: string): boolean {
  const textArea = document.createElement('textarea')
  textArea.value = text
  textArea.setAttribute('readonly', '')
  textArea.style.position = 'fixed'
  textArea.style.top = '0'
  textArea.style.left = '0'
  textArea.style.width = '1px'
  textArea.style.height = '1px'
  textArea.style.padding = '0'
  textArea.style.border = '0'
  textArea.style.opacity = '0'

  document.body.appendChild(textArea)

  const selection = document.getSelection()
  const selectedRange = selection && selection.rangeCount > 0
    ? selection.getRangeAt(0)
    : null

  textArea.focus()
  textArea.select()
  textArea.setSelectionRange(0, textArea.value.length)

  let copied = false
  try {
    copied = document.execCommand('copy')
  } finally {
    document.body.removeChild(textArea)

    if (selectedRange && selection) {
      selection.removeAllRanges()
      selection.addRange(selectedRange)
    }
  }

  return copied
}

export async function copyTextToClipboard(text: string): Promise<void> {
  if (window.isSecureContext && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text)
      return
    } catch {
      // Fall through to the legacy copy path below. Some browsers reject the
      // async Clipboard API even inside a user gesture.
    }
  }

  if (copyWithTextarea(text)) {
    return
  }

  throw new Error('当前浏览器不允许访问剪贴板，请使用 HTTPS 或通过 localhost/SSH 隧道打开管理后台后重试')
}
