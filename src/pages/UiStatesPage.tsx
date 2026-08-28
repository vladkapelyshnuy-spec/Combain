import { ErrorState, LoadingState } from "../components/ui";

export function UiStatesPage() {
  return (
    <>
      <header className="page-header">
        <p className="eyebrow">Shared primitives</p>
        <h1>Predictable states.</h1>
        <p className="lead">
          Reusable feedback components keep long-running desktop operations clear and
          consistent across future feature screens.
        </p>
      </header>

      <div className="state-grid">
        <LoadingState
          title="Preparing workspace"
          description="Please wait while the requested operation is in progress."
        />
        <ErrorState
          title="Something went wrong"
          description="The operation could not be completed. Review the message and try again."
          onRetry={() => undefined}
        />
      </div>
    </>
  );
}
