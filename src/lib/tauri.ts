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

export type AppErrorKind =
  | 'session_expired'
  | 'rate_limited'
  | 'network'
  | 'decode'
  | 'db'
  | 'io'

export interface AppError {
  kind: AppErrorKind
  message: string
}

export function isAppError(e: unknown): e is AppError {
  return (
    typeof e === 'object' &&
    e !== null &&
    'kind' in e &&
    'message' in e &&
    typeof (e as { kind: unknown }).kind === 'string'
  )
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
