import type { DiffResult } from '../lib/tauri'

interface DiffBannerProps {
  diff: DiffResult
}

function formatDate(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return d.toLocaleString()
}

export function DiffBanner({ diff }: DiffBannerProps) {
  const gained = diff.new_followers.length
  const lost = diff.lost_followers.length
  if (gained === 0 && lost === 0) return null

  const since = formatDate(diff.since)

  return (
    <div className="diff-banner" role="status">
      <div className="diff-banner-counts">
        <span className="diff-gain">+{gained} new follower{gained === 1 ? '' : 's'}</span>
        <span className="diff-loss">−{lost} unfollower{lost === 1 ? '' : 's'}</span>
      </div>
      {since && <div className="diff-banner-since">since {since}</div>}
    </div>
  )
}
