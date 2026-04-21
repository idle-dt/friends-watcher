export function RateLimitedBanner() {
  return (
    <div className="banner banner-rate-limited" role="alert">
      <div className="banner-body">
        <strong>Instagram is rate-limiting requests.</strong>
        <span>Try again in a few minutes.</span>
      </div>
    </div>
  )
}
