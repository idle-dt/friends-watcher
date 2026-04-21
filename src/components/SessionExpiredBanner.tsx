interface SessionExpiredBannerProps {
  onReturnToLogin: () => void
}

export function SessionExpiredBanner({ onReturnToLogin }: SessionExpiredBannerProps) {
  return (
    <div className="banner banner-session-expired" role="alert">
      <div className="banner-body">
        <strong>Session expired.</strong>
        <span>Please log in again to continue.</span>
      </div>
      <button type="button" className="banner-action" onClick={onReturnToLogin}>
        Log in again
      </button>
    </div>
  )
}
