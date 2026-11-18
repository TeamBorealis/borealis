import { Component, type ReactNode } from "react";

interface ErrorBoundaryProps {
  /** What failed, for the note shown in its place ("Wizard spellcasting"). */
  label: string;
  /** A new value (the next snapshot) tries the part again. */
  resetKey?: unknown;
  children: ReactNode;
}

interface ErrorBoundaryState {
  failed: boolean;
  resetKey: unknown;
}

/**
 * GitHub #68: one part of a page that throws while rendering shows a short note in its place instead of unmounting the
 * whole app (a blank window). The rest of the page keeps working; the part is tried again when `resetKey` changes.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { failed: false, resetKey: this.props.resetKey };

  static getDerivedStateFromError(): Partial<ErrorBoundaryState> {
    return { failed: true };
  }

  static getDerivedStateFromProps(props: ErrorBoundaryProps, state: ErrorBoundaryState): Partial<ErrorBoundaryState> | null {
    return props.resetKey === state.resetKey ? null : { failed: false, resetKey: props.resetKey };
  }

  override render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return <p className="error-text" role="alert">{this.props.label} could not be shown. The rest of the page still works.</p>;
  }
}
