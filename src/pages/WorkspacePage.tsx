import { Button, Panel } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { AppRoute } from "../routing/routes";

type WorkspacePageProps = {
  route: AppRoute;
};

export function WorkspacePage({ route }: WorkspacePageProps) {
  return (
    <div className="workspace-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true">
            <SectionIcon name={route.icon} />
          </span>
          <div>
            <p className="eyebrow">Application workspace</p>
            <h1>{route.label}</h1>
            <p className="lead">{route.description}</p>
          </div>
        </div>
        <span className="phase-badge">Coming in a later phase</span>
      </header>

      <Panel
        title={`${route.label} workspace`}
        actions={<Button disabled>Not available yet</Button>}
      >
        <div className="workspace-placeholder">
          <span className="placeholder-mark" aria-hidden="true">
            <SectionIcon name={route.icon} />
          </span>
          <div>
            <h2>Ready for feature integration</h2>
            <p>
              This section is part of the application structure. Its functionality
              will be implemented in the corresponding documented phase.
            </p>
          </div>
        </div>
      </Panel>
    </div>
  );
}
