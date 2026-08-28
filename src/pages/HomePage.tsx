import { Button, Panel } from "../components/ui";

export function HomePage() {
  return (
    <>
      <header className="page-header">
        <p className="eyebrow">UI Bootstrap</p>
        <h1>Create with clarity.</h1>
        <p className="lead">
          The Combain Studio application shell is ready for the project workflow.
          Feature screens and backend integrations will be added in later phases.
        </p>
      </header>

      <div className="panel-grid">
        <Panel title="Application shell" actions={<Button disabled>New project</Button>}>
          The shared layout, design tokens, responsive behavior, and keyboard focus
          styles are in place.
        </Panel>
        <Panel
          title="Phase boundary"
          actions={<Button variant="secondary" disabled>Open workspace</Button>}
        >
          Project, storyboard, media, timeline, and export features remain disabled
          until their documented phases.
        </Panel>
      </div>
    </>
  );
}
