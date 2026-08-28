import type { MouseEvent, ReactNode } from "react";
import type { AppPath } from "../routing/useAppRouter";

type AppShellProps = {
  children: ReactNode;
  currentPath: AppPath;
  onNavigate: (path: AppPath) => void;
};

type NavLinkProps = {
  children: ReactNode;
  currentPath: AppPath;
  path: AppPath;
  onNavigate: (path: AppPath) => void;
};

function NavLink({ children, currentPath, path, onNavigate }: NavLinkProps) {
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    onNavigate(path);
  };

  return (
    <a
      className="nav-link"
      href={path}
      aria-current={currentPath === path ? "page" : undefined}
      onClick={handleClick}
    >
      {children}
    </a>
  );
}

export function AppShell({ children, currentPath, onNavigate }: AppShellProps) {
  return (
    <div className="app-shell">
      <header className="topbar">
        <a
          className="brand"
          href="/"
          onClick={(event) => {
            event.preventDefault();
            onNavigate("/");
          }}
        >
          <span className="brand-mark" aria-hidden="true">CS</span>
          <span>Combain Studio</span>
        </a>

        <nav className="nav" aria-label="Primary navigation">
          <NavLink currentPath={currentPath} path="/" onNavigate={onNavigate}>
            Home
          </NavLink>
          <NavLink currentPath={currentPath} path="/ui-states" onNavigate={onNavigate}>
            UI States
          </NavLink>
        </nav>
      </header>

      <main className="workspace">{children}</main>
    </div>
  );
}
