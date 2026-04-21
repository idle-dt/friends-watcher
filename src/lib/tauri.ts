import { invoke } from '@tauri-apps/api/core'

export type RelationshipStatus = 'mutual' | 'fan' | 'ghost' | 'new' | 'lost'

export interface UserRow {
  ig_user_id: string
  username: string
  full_name: string | null
  is_verified: boolean
  profile_pic_url: string | null
}

export interface Relationship {
  ig_user_id: string
  username: string
  full_name: string | null
  is_verified: boolean
  profile_pic_url: string | null
  follows_you: boolean
  you_follow: boolean
  status: RelationshipStatus
}

export interface SessionState {
  logged_in: boolean
  username: string | null
  last_sync_at: string | null
}

export interface SyncResult {
  new_followers: UserRow[]
  lost_followers: UserRow[]
  total_followers: number
  total_following: number
}

export interface DiffResult {
  since: string | null
  new_followers: UserRow[]
  lost_followers: UserRow[]
}

export type AppError =
  | { kind: 'session_expired'; message: string }
  | { kind: 'rate_limited'; message: string }
  | { kind: 'network'; message: string }
  | { kind: 'decode'; message: string }
  | { kind: 'db'; message: string }
  | { kind: 'io'; message: string }

export type AppErrorKind = AppError['kind']

const APP_ERROR_KINDS: readonly AppErrorKind[] = [
  'session_expired',
  'rate_limited',
  'network',
  'decode',
  'db',
  'io',
]

export function isAppError(e: unknown): e is AppError {
  if (typeof e !== 'object' || e === null) return false
  const obj = e as { kind?: unknown; message?: unknown }
  return (
    typeof obj.kind === 'string' &&
    (APP_ERROR_KINDS as readonly string[]).includes(obj.kind) &&
    typeof obj.message === 'string'
  )
}

export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return e
  const message =
    e instanceof Error ? e.message : typeof e === 'string' ? e : String(e)
  return { kind: 'network', message }
}

export function getSessionState(): Promise<SessionState> {
  return invoke<SessionState>('get_session_state')
}

export function syncNow(): Promise<SyncResult> {
  return invoke<SyncResult>('sync_now')
}

export function getLatestRelationships(): Promise<Relationship[]> {
  return invoke<Relationship[]>('get_latest_relationships')
}

export function getDiffSincePrevious(): Promise<DiffResult> {
  return invoke<DiffResult>('get_diff_since_previous')
}

export function openProfile(username: string): Promise<void> {
  return invoke<void>('open_profile', { username })
}
