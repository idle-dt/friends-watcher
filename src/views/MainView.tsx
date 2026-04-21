import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getDiffSincePrevious,
  getLatestRelationships,
  isAppError,
  syncNow,
  type AppError,
  type DiffResult,
  type Relationship,
  type SessionState,
} from '../lib/tauri'
import { DiffBanner } from '../components/DiffBanner'
import { RelationshipRow } from '../components/RelationshipRow'
import { StatusEmpty } from '../components/StatusEmpty'

const EMPTY_DIFF: DiffResult = {
  since: null,
  new_followers: [],
  lost_followers: [],
}

type FilterKey = 'all' | 'mutual' | 'fan' | 'ghost'

interface MainViewProps {
  session: SessionState
  onSessionExpired: () => void
}

export function MainView({ session, onSessionExpired }: MainViewProps) {
  const [relationships, setRelationships] = useState<Relationship[]>([])
  const [diff, setDiff] = useState<DiffResult>(EMPTY_DIFF)
  const [syncing, setSyncing] = useState(false)
  const [loadingInitial, setLoadingInitial] = useState(true)
  const [error, setError] = useState<AppError | null>(null)
  const [filter, setFilter] = useState<FilterKey>('all')

  const refresh = useCallback(async () => {
    const [rels, d] = await Promise.all([
      getLatestRelationships(),
      getDiffSincePrevious(),
    ])
    setRelationships(rels)
    setDiff(d)
  }, [])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        await refresh()
      } catch (e) {
        if (!cancelled && isAppError(e)) setError(e)
      } finally {
        if (!cancelled) setLoadingInitial(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [refresh])

  const handleSync = useCallback(async () => {
    setSyncing(true)
    setError(null)
    try {
      await syncNow()
      await refresh()
    } catch (e) {
      if (isAppError(e)) {
        setError(e)
        if (e.kind === 'session_expired') onSessionExpired()
      } else {
        setError({ kind: 'network', message: String(e) })
      }
    } finally {
      setSyncing(false)
    }
  }, [refresh, onSessionExpired])

  const filtered = useMemo(() => {
    if (filter === 'all') return relationships
    return relationships.filter((r) => r.status === filter)
  }, [relationships, filter])

  const counts = useMemo(() => {
    const base = { mutual: 0, fan: 0, ghost: 0 }
    for (const r of relationships) {
      if (r.status in base) base[r.status as keyof typeof base] += 1
    }
    return base
  }, [relationships])

  const lastSyncText = session.last_sync_at
    ? new Date(session.last_sync_at).toLocaleString()
    : 'never'

  return (
    <div className="view main-view">
      <header className="view-header">
        <div className="main-header-left">
          <h1>Friends Watcher</h1>
          <p className="subtitle">
            {session.username ? `@${session.username}` : 'Connected'} · last sync:{' '}
            {lastSyncText}
          </p>
        </div>
        <button
          className="sync-button"
          onClick={handleSync}
          disabled={syncing}
          type="button"
        >
          {syncing ? 'Syncing…' : 'Sync now'}
        </button>
      </header>

      <DiffBanner diff={diff} />

      {error && error.kind !== 'session_expired' && (
        <div className={`error-banner error-${error.kind}`} role="alert">
          {error.message}
        </div>
      )}

      {syncing && (
        <div className="sync-progress" role="status">
          Checking followers — this can take up to a minute.
        </div>
      )}

      {loadingInitial ? (
        <div className="main-loading">Loading…</div>
      ) : relationships.length === 0 ? (
        <StatusEmpty />
      ) : (
        <>
          <div className="filters" role="tablist">
            <FilterButton current={filter} value="all" setFilter={setFilter}>
              All ({relationships.length})
            </FilterButton>
            <FilterButton current={filter} value="mutual" setFilter={setFilter}>
              Mutual ({counts.mutual})
            </FilterButton>
            <FilterButton current={filter} value="fan" setFilter={setFilter}>
              Fans ({counts.fan})
            </FilterButton>
            <FilterButton current={filter} value="ghost" setFilter={setFilter}>
              Ghosts ({counts.ghost})
            </FilterButton>
          </div>

          <div className="table-wrap">
            <table className="relationships-table">
              <thead>
                <tr>
                  <th className="col-avatar"></th>
                  <th className="col-user">User</th>
                  <th className="col-badges">Relationship</th>
                  <th className="col-status">Status</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((r) => (
                  <RelationshipRow key={r.ig_user_id} relationship={r} />
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  )
}

interface FilterButtonProps {
  current: FilterKey
  value: FilterKey
  setFilter: (v: FilterKey) => void
  children: React.ReactNode
}

function FilterButton({ current, value, setFilter, children }: FilterButtonProps) {
  const active = current === value
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={`filter-button${active ? ' active' : ''}`}
      onClick={() => setFilter(value)}
    >
      {children}
    </button>
  )
}
