import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getDiffSincePrevious,
  getLatestRelationships,
  getSessionState,
  syncNow,
  syncProgress,
  toAppError,
  type AppError,
  type DiffResult,
  type Relationship,
  type SessionState,
  type SyncProgress,
} from '../lib/tauri'
import { DiffBanner } from '../components/DiffBanner'
import { RelationshipRow } from '../components/RelationshipRow'
import { StatusEmpty } from '../components/StatusEmpty'
import { SessionExpiredBanner } from '../components/SessionExpiredBanner'
import { RateLimitedBanner } from '../components/RateLimitedBanner'

const EMPTY_DIFF: DiffResult = {
  since: null,
  new_followers: [],
  lost_followers: [],
}

type FilterKey = 'all' | 'mutual' | 'fan' | 'ghost'

interface MainViewProps {
  session: SessionState
  onSessionExpired: () => void
  onSessionChanged: (session: SessionState) => void
}

export function MainView({
  session,
  onSessionExpired,
  onSessionChanged,
}: MainViewProps) {
  const [relationships, setRelationships] = useState<Relationship[]>([])
  const [diff, setDiff] = useState<DiffResult>(EMPTY_DIFF)
  const [syncing, setSyncing] = useState(false)
  const [progress, setProgress] = useState<SyncProgress | null>(null)
  const [followersFetched, setFollowersFetched] = useState<number | null>(null)
  const [followingFetched, setFollowingFetched] = useState<number | null>(null)
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
        if (!cancelled) setError(toAppError(e))
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
    setProgress(null)
    setFollowersFetched(null)
    setFollowingFetched(null)
    setError(null)
    let unlisten: (() => void) | undefined
    try {
      unlisten = await syncProgress((p) => {
        setProgress(p)
        if (p.phase === 'followers' && p.fetched !== undefined) {
          setFollowersFetched(p.fetched)
        } else if (p.phase === 'following' && p.fetched !== undefined) {
          setFollowingFetched(p.fetched)
        }
      })
      await syncNow()
      await refresh()
      try {
        const next = await getSessionState()
        onSessionChanged(next)
      } catch (e) {
        console.warn('failed to refresh session after sync', e)
      }
    } catch (e) {
      setError(toAppError(e))
    } finally {
      if (unlisten) unlisten()
      setSyncing(false)
      setProgress(null)
      setFollowersFetched(null)
      setFollowingFetched(null)
    }
  }, [refresh, onSessionChanged])

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

  const changeByUserId = useMemo(() => {
    const map = new Map<string, 'new' | 'unfollowed'>()
    for (const u of diff.new_followers) map.set(u.ig_user_id, 'new')
    for (const u of diff.lost_followers) map.set(u.ig_user_id, 'unfollowed')
    return map
  }, [diff])

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

      {error?.kind === 'session_expired' && (
        <SessionExpiredBanner onReturnToLogin={onSessionExpired} />
      )}
      {error?.kind === 'rate_limited' && <RateLimitedBanner />}
      {error &&
        error.kind !== 'session_expired' &&
        error.kind !== 'rate_limited' && (
          <div className={`error-banner error-${error.kind}`} role="alert">
            {error.message}
          </div>
        )}

      {syncing && (
        <div className="sync-progress" role="status">
          {progressMessage(progress, followersFetched, followingFetched)}
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
                  <RelationshipRow
                    key={r.ig_user_id}
                    relationship={r}
                    change={changeByUserId.get(r.ig_user_id) ?? null}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  )
}

function progressMessage(
  progress: SyncProgress | null,
  followersFetched: number | null,
  followingFetched: number | null,
): string {
  if (!progress) return 'Starting sync…'
  switch (progress.phase) {
    case 'profile':
      return 'Resolving profile…'
    case 'followers':
    case 'following': {
      const parts: string[] = []
      if (followersFetched !== null) parts.push(`followers ${followersFetched}`)
      if (followingFetched !== null) parts.push(`following ${followingFetched}`)
      if (parts.length === 0) return 'Fetching followers and following…'
      return `Fetching ${parts.join(' / ')}…`
    }
    case 'writing':
      return 'Saving snapshot…'
    default:
      return 'Syncing…'
  }
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
