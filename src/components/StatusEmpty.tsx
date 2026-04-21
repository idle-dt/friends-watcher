interface StatusEmptyProps {
  title?: string
  message?: string
}

export function StatusEmpty({
  title = 'No snapshot yet',
  message = 'Click Sync to fetch your followers and following for the first time.',
}: StatusEmptyProps) {
  return (
    <div className="status-empty" role="status">
      <h2>{title}</h2>
      <p>{message}</p>
    </div>
  )
}
