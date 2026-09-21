/** Apple-style activity indicator — bare radial ticks (asterisk look). */
export default function AppleSpinner({
  size = 22,
  className = "",
  color,
}: {
  size?: number;
  className?: string;
  /** CSS color; defaults to turquoise. */
  color?: string;
}) {
  const ticks = 12;
  return (
    <span
      className={`ms-apple-spin ${className}`.trim()}
      style={{
        width: size,
        height: size,
        ...(color ? { ["--ms-spin-color" as string]: color } : {}),
      }}
      role="status"
      aria-label="Loading"
    >
      {Array.from({ length: ticks }, (_, i) => (
        <span
          key={i}
          className="ms-apple-spin-tick"
          style={{
            transform: `rotate(${(i * 360) / ticks}deg)`,
            animationDelay: `${(-(ticks - i) / ticks) * 0.85}s`,
          }}
        />
      ))}
    </span>
  );
}
