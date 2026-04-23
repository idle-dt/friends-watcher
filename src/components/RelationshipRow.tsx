import { useEffect, useState } from 'react'
import type { Relationship, RelationshipStatus } from '../lib/tauri'
import { getAvatar, openProfile } from '../lib/tauri'

const STATUS_LABEL: Record<RelationshipStatus, string> = {
  mutual: 'Mutual',
  fan: 'Fan',
  ghost: 'Ghost',
  new: 'New',
  lost: 'Lost',
}

interface AvatarState {
  key: string
  objectUrl: string | null
  broken: boolean
}

const INITIAL_AVATAR: AvatarState = { key: '', objectUrl: null, broken: false }

interface RelationshipRowProps {
  relationship: Relationship
  change: 'new' | 'unfollowed' | null
}

export function RelationshipRow({ relationship: r, change }: RelationshipRowProps) {
  const [avatar, setAvatar] = useState<AvatarState>(INITIAL_AVATAR)
  const currentKey = `${r.ig_user_id}|${r.profile_pic_url ?? ''}`

  useEffect(() => {
    const url = r.profile_pic_url
    if (!url) return

    let cancelled = false
    let created: string | null = null

    getAvatar(r.ig_user_id, url)
      .then((bytes) => {
        if (cancelled) return
        const blob = new Blob([new Uint8Array(bytes)])
        created = URL.createObjectURL(blob)
        setAvatar({ key: currentKey, objectUrl: created, broken: false })
      })
      .catch(() => {
        if (cancelled) return
        setAvatar({ key: currentKey, objectUrl: null, broken: true })
      })

    return () => {
      cancelled = true
      if (created) URL.revokeObjectURL(created)
    }
  }, [r.ig_user_id, r.profile_pic_url, currentKey])

  const handleOpen = (event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    openProfile(r.username).catch(() => {
      // error surfacing is the parent's job; swallow here to avoid unhandled rejections
    })
  }

  const matchesCurrent = avatar.key === currentKey
  const showImage = matchesCurrent && avatar.objectUrl !== null && !avatar.broken

  return (
    <tr className={`relationship-row${change ? ` row-change-${change}` : ''}`}>
      <td className="col-avatar">
        {showImage ? (
          <img
            src={avatar.objectUrl as string}
            alt=""
            className="avatar"
            width={32}
            height={32}
            loading="lazy"
            onError={() => {
              const stale = avatar.objectUrl
              if (stale) URL.revokeObjectURL(stale)
              setAvatar({ key: currentKey, objectUrl: null, broken: true })
            }}
          />
        ) : (
          <span className="avatar avatar-fallback" aria-hidden="true">
            {r.username.slice(0, 1).toUpperCase()}
          </span>
        )}
      </td>
      <td className="col-user">
        <a
          href={`https://instagram.com/${r.username}`}
          onClick={handleOpen}
          className="username"
        >
          @{r.username}
          {r.is_verified && (
            <span className="verified" title="Verified" aria-label="verified">
              {' '}
              ✓
            </span>
          )}
        </a>
        {r.full_name && <div className="full-name">{r.full_name}</div>}
      </td>
      <td className="col-badges">
        {r.follows_you && <span className="badge badge-followsyou">Follows you</span>}
        {r.you_follow && <span className="badge badge-youfollow">You follow</span>}
        {change === 'new' && <span className="badge badge-new">New</span>}
        {change === 'unfollowed' && (
          <span className="badge badge-unfollowed">Unfollowed you</span>
        )}
      </td>
      <td className="col-status">
        <span className={`status status-${r.status}`}>{STATUS_LABEL[r.status]}</span>
      </td>
    </tr>
  )
}
