import { useEffect, useState } from 'react'
import { startIgLogin } from '../lib/tauri'

export function LoginView() {
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    startIgLogin().catch((e) => {
      setError(e instanceof Error ? e.message : String(e))
    })
  }, [])

  return (
    <div className="view login-view">
      <header className="view-header">
        <h1>Friends Watcher</h1>
        <p>Log in to Instagram to continue. Your session stays local.</p>
      </header>
      <div className="login-webview-slot">
        <div
          className="login-webview-placeholder"
          aria-busy={error === null}
          role="status"
        >
          {error ? (
            <span className="error">Could not open Instagram: {error}</span>
          ) : (
            <span>Opening Instagram login…</span>
          )}
        </div>
      </div>
    </div>
  )
}
