import type { ButtonHTMLAttributes, ReactNode } from "react";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
};

export function Button({ className = "", type = "button", variant = "primary", ...props }: ButtonProps) {
  return (
    <button
      className={`button button-${variant} ${className}`.trim()}
      type={type}
      {...props}
    />
  );
}

type PanelProps = {
  actions?: ReactNode;
  children: ReactNode;
  title: string;
};

export function Panel({ actions, children, title }: PanelProps) {
  return (
    <section className="panel">
      <h2>{title}</h2>
      <div className="panel-copy">{children}</div>
      {actions ? <div className="panel-actions">{actions}</div> : null}
    </section>
  );
}

type StateProps = {
  description: string;
  title: string;
};

export function LoadingState({ description, title }: StateProps) {
  return (
    <section className="state-card" aria-live="polite" aria-busy="true">
      <span className="state-icon" aria-hidden="true">
        <span className="spinner" />
      </span>
      <h2>{title}</h2>
      <p className="state-copy">{description}</p>
    </section>
  );
}

type ErrorStateProps = StateProps & {
  onRetry?: () => void;
};

export function ErrorState({ description, onRetry, title }: ErrorStateProps) {
  return (
    <section className="state-card" role="alert">
      <span className="state-icon state-icon-error" aria-hidden="true">!</span>
      <h2>{title}</h2>
      <p className="state-copy">{description}</p>
      {onRetry ? (
        <div className="panel-actions">
          <Button variant="secondary" onClick={onRetry}>Try again</Button>
        </div>
      ) : null}
    </section>
  );
}
