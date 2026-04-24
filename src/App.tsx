import { useCallback, useEffect, useState } from 'react'
import { LoginView } from './views/LoginView'
import { MainView } from './views/MainView'
import { getSessionState, type SessionState } from './lib/tauri'
import './App.css'

type ViewState =
  | { kind: 'loading' }
  | { kind: 'login' }
  | { kind: 'main'; session: SessionState }
  | { kind: 'error'; message: string }

function App() {
  const [view, setView] = useState<ViewState>({ kind: 'loading' })

  const refreshSession = useCallback(async () => {
    try {
      const session = await getSessionState()
      setView(
        session.logged_in
          ? { kind: 'main', session }
          : { kind: 'login' },
      )
    } catch (e) {
      setView({ kind: 'error', message: e instanceof Error ? e.message : String(e) })
    }
  }, [])

  useEffect(() => {
    // refreshSession awaits before setState, so it does not synchronously update state.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    refreshSession()
  }, [refreshSession])

  const handleSessionExpired = useCallback(() => {
    setView({ kind: 'login' })
  }, [])

  const handleSessionChanged = useCallback((session: SessionState) => {
    setView((prev) => (prev.kind === 'main' ? { kind: 'main', session } : prev))
  }, [])

  if (view.kind === 'loading') {
    return (
      <div className="app-loading" role="status">
        Starting…
      </div>
    )
  }

  if (view.kind === 'error') {
    return (
      <div className="app-error" role="alert">
        <h1>Something went wrong</h1>
        <p>{view.message}</p>
      </div>
    )
  }

  if (view.kind === 'login') {
    return <LoginView />
  }

  return (
    <MainView
      session={view.session}
      onSessionExpired={handleSessionExpired}
      onSessionChanged={handleSessionChanged}
    />
  )
}

export default App
