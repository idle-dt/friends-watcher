import type { Relationship, RelationshipStatus } from '../lib/tauri'
import { openProfile } from '../lib/tauri'

const STATUS_LABEL: Record<RelationshipStatus, string> = {
  mutual: 'Mutual',
  fan: 'Fan',
  ghost: 'Ghost',
  new: 'New',
  lost: 'Lost',
}

interface RelationshipRowProps {
  relationship: Relationship
}

export function RelationshipRow({ relationship: r }: RelationshipRowProps) {
  const handleOpen = (event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    openProfile(r.username).catch(() => {
      // error surfacing is the parent's job; swallow here to avoid unhandled rejections
    })
  }

  return (
    <tr className="relationship-row">
      <td className="col-avatar">
        {r.profile_pic_url ? (
          <img
            src={r.profile_pic_url}
            alt=""
            className="avatar"
            width={32}
            height={32}
            loading="lazy"
            referrerPolicy="no-referrer"
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
      </td>
      <td className="col-status">
        <span className={`status status-${r.status}`}>{STATUS_LABEL[r.status]}</span>
      </td>
    </tr>
  )
}
