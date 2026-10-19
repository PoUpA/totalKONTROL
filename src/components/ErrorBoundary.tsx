import { Component, type ReactNode } from "react";
import { useI18n } from "../i18n";

function Fallback({ error }: { error: Error }) {
  const { t } = useI18n();
  return (
    <main className="app">
      <section className="card" role="alert">
        <h2>{t("error.title")}</h2>
        <pre className="error">{error.message}</pre>
        <button className="primary" onClick={() => window.location.reload()}>
          {t("error.reload")}
        </button>
      </section>
    </main>
  );
}

/**
 * Shows the error instead of a blank window when rendering throws. The
 * padKONTROL keeps working meanwhile: the Rust core does not depend on it.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    return this.state.error ? <Fallback error={this.state.error} /> : this.props.children;
  }
}
