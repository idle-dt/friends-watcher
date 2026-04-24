import { useState } from 'react'
import { startIgLogin } from '../lib/tauri'

export function LoginView() {
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)

  const handleLogin = async () => {
    setError(null)
    setPending(true)
    try {
      await startIgLogin()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setPending(false)
    }
  }

  return (
    <div className="view login-view">
      <header className="view-header">
        <h1>Friends Watcher</h1>
      </header>
      <div className="login-webview-slot">
        <div className="landing">
          <h2 className="landing-hero">Know who follows you back.</h2>
          <p className="landing-lede">
            Friends Watcher monitors who follows you on Instagram and tracks
            changes over time. Everything stays on your machine — no
            third-party servers, no accounts, no telemetry.
          </p>
          <button
            type="button"
            className="login-cta"
            onClick={handleLogin}
            disabled={pending}
          >
            Log in with Instagram
          </button>
          {pending && !error && (
            <p className="landing-status" role="status">
              Opening Instagram login…
            </p>
          )}
          {error && (
            <p className="landing-status error" role="alert">
              Could not open Instagram: {error}
            </p>
          )}
        </div>
      </div>
    </div>
  )
}
