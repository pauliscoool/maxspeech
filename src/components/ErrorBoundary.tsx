import { Component, type ErrorInfo, type ReactNode } from "react";

type Props = {
  children: ReactNode;
  /** Shown when a child throws during render. */
  label?: string;
};

type State = { error: Error | null };

/** Keeps a React render crash from leaving WebView2 on a blank white screen. */
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("MaxSpeech UI error:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    const title = this.props.label ?? "Something went wrong loading MaxSpeech.";
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-3 bg-[var(--ms-bg)] text-[var(--ms-text)] p-8">
        <p className="text-sm font-medium">{title}</p>
        <p className="text-xs text-[var(--ms-text-dim)] max-w-md text-center break-words">
          {error.message || "Unknown error"}
        </p>
        <button
          type="button"
          className="btn-primary px-4 py-2 text-xs"
          onClick={() => this.setState({ error: null })}
        >
          Try again
        </button>
      </div>
    );
  }
}
