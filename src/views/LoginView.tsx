import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Webview, getAllWebviews } from '@tauri-apps/api/webview'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { getSessionState } from '../lib/tauri'

const IG_LOGIN_URL = 'https://www.instagram.com/accounts/login/'
const IG_WEBVIEW_LABEL = 'ig'

// Must match IG_WEBVIEW_USER_AGENT in src-tauri/src/cookies.rs.
const IG_UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15'

const POLL_INTERVAL_MS = 2000

interface LoginViewProps {
  onLogin: () => void
}

async function disposeExistingIgWebview(): Promise<void> {
  const existing = await getAllWebviews()
  for (const w of existing) {
    if (w.label === IG_WEBVIEW_LABEL) {
      try {
        await w.close()
      } catch {
        // ignore — webview may already be gone
      }
    }
  }
}

export function LoginView({ onLogin }: LoginViewProps) {
  const slotRef = useRef<HTMLDivElement>(null)
  const webviewRef = useRef<Webview | null>(null)
  const [status, setStatus] = useState<'idle' | 'loading' | 'ready' | 'error'>(
    'idle',
  )
  const [errorMessage, setErrorMessage] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    let cleanupResize: (() => void) | null = null

    async function attach() {
      setStatus('loading')
      try {
        await disposeExistingIgWebview()
        if (cancelled) return

        const slot = slotRef.current
        if (!slot) return

        const rect = slot.getBoundingClientRect()
        const win = getCurrentWindow()
        const webview = new Webview(win, IG_WEBVIEW_LABEL, {
          url: IG_LOGIN_URL,
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          width: Math.max(1, Math.round(rect.width)),
          height: Math.max(1, Math.round(rect.height)),
          userAgent: IG_UA,
          acceptFirstMouse: true,
        })

        await new Promise<void>((resolve, reject) => {
          let settled = false
          webview.once('tauri://created', () => {
            if (settled) return
            settled = true
            resolve()
          })
          webview.once('tauri://error', (e) => {
            if (settled) return
            settled = true
            reject(new Error(String(e.payload ?? 'webview error')))
          })
        })

        if (cancelled) {
          await webview.close().catch(() => {})
          return
        }

        webviewRef.current = webview
        setStatus('ready')

        const syncBounds = () => {
          const s = slotRef.current
          if (!s || !webviewRef.current) return
          const r = s.getBoundingClientRect()
          webviewRef.current
            .setPosition(new LogicalPosition(Math.round(r.left), Math.round(r.top)))
            .catch(() => {})
          webviewRef.current
            .setSize(
              new LogicalSize(
                Math.max(1, Math.round(r.width)),
                Math.max(1, Math.round(r.height)),
              ),
            )
            .catch(() => {})
        }

        const ro = new ResizeObserver(syncBounds)
        ro.observe(slot)
        window.addEventListener('resize', syncBounds)
        cleanupResize = () => {
          ro.disconnect()
          window.removeEventListener('resize', syncBounds)
        }
      } catch (e) {
        if (!cancelled) {
          setStatus('error')
          setErrorMessage(e instanceof Error ? e.message : String(e))
        }
      }
    }

    attach()

    return () => {
      cancelled = true
      cleanupResize?.()
      const wv = webviewRef.current
      webviewRef.current = null
      if (wv) {
        wv.close().catch(() => {})
      }
    }
  }, [])

  useEffect(() => {
    let stopped = false
    let timer: number | null = null

    const poll = async () => {
      if (stopped) return
      try {
        const state = await getSessionState()
        if (!stopped && state.logged_in) {
          onLogin()
          return
        }
      } catch {
        // retry on next tick
      }
      if (!stopped) {
        timer = window.setTimeout(poll, POLL_INTERVAL_MS)
      }
    }

    poll()

    return () => {
      stopped = true
      if (timer !== null) window.clearTimeout(timer)
    }
  }, [onLogin])

  return (
    <div className="view login-view">
      <header className="view-header">
        <h1>Friends Watcher</h1>
        <p>Log in to Instagram to continue. Your session stays local.</p>
      </header>
      <div ref={slotRef} className="login-webview-slot">
        {status !== 'ready' && (
          <div className="login-webview-placeholder" aria-busy={status === 'loading'}>
            {status === 'loading' && <span>Loading Instagram…</span>}
            {status === 'error' && (
              <span className="error">
                Could not load Instagram: {errorMessage ?? 'unknown error'}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
