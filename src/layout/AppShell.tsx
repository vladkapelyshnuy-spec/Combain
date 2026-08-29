import type { MouseEvent, ReactNode } from "react";
import { findRoute, type AppPath } from "../routing/routes";
import { Sidebar } from "./Sidebar";

type AppShellProps = {
  children: ReactNode;
  currentPath: AppPath;
  onNavigate: (path: AppPath) => void;
};

export function AppShell({ children, currentPath, onNavigate }: AppShellProps) {
  const currentRoute = findRoute(currentPath);

  return (
    <div className="app-shell">
      <header className="topbar">
        <a
          className="brand"
          href="/projects"
          onClick={(event: MouseEvent<HTMLAnchorElement>) => {
            event.preventDefault();
            onNavigate("/projects");
          }}
        >
          <span className="brand-mark" aria-hidden="true">CS</span>
          <span>Combain Studio</span>
        </a>

        <div className="topbar-context" aria-label="Current workspace">
          <span className="topbar-context-label">Current section</span>
          <strong>{currentRoute.label}</strong>
        </div>

        <div className="project-context" aria-label="Selected project">
          <span className="project-avatar" aria-hidden="true">NP</span>
          <span className="project-copy">
            <span>No project selected</span>
            <small>Local workspace</small>
          </span>
        </div>
      </header>

      <div className="app-body">
        <Sidebar currentPath={currentPath} onNavigate={onNavigate} />
        <main className="workspace" tabIndex={-1}>{children}</main>
      </div>
    </div>
  );
}
