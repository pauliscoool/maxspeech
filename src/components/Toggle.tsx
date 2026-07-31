/** Smooth turquoise toggle switch. */
export default function Toggle({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={onChange}
      className={`toggle-switch ${checked ? "toggle-on" : "toggle-off"} ${
        disabled ? "opacity-40 pointer-events-none" : ""
      }`}
    >
      <span className="toggle-knob" />
    </button>
  );
}
