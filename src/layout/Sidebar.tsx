import type { MouseEvent } from "react";
import { SectionIcon } from "../components/SectionIcon";
import { appRoutes, type AppPath } from "../routing/routes";

type SidebarProps = {
  currentPath: AppPath;
  onNavigate: (path: AppPath) => void;
};

export function Sidebar({ currentPath, onNavigate }: SidebarProps) {
  const handleNavigate = (event: MouseEvent<HTMLAnchorElement>, path: AppPath) => {
    event.preventDefault();
    onNavigate(path);
  };

  return (
    <aside className="sidebar">
      <p className="sidebar-label">Workspace</p>
      <nav className="sidebar-nav" aria-label="Workspace navigation">
        {appRoutes.map((route) => (
          <a
            className="sidebar-link"
            href={route.path}
            aria-current={currentPath === route.path ? "page" : undefined}
            key={route.path}
            onClick={(event) => handleNavigate(event, route.path)}
          >
            <SectionIcon className="sidebar-icon" name={route.icon} />
            <span>{route.label}</span>
          </a>
        ))}
      </nav>

      <div className="sidebar-footer" aria-label="Application status">
        <span className="status-dot" aria-hidden="true" />
        <span>UI workspace</span>
        <span className="sidebar-version">v0.1</span>
      </div>
    </aside>
  );
}
