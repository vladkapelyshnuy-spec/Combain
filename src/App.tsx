import { useState } from "react";
import "./App.css";
import { AppShell } from "./layout/AppShell";
import type { Project } from "./models/project";
import { ProjectsPage } from "./pages/ProjectsPage";
import { WorkspacePage } from "./pages/WorkspacePage";
import { findRoute } from "./routing/routes";
import { useAppRouter } from "./routing/useAppRouter";

function App() {
  const { currentPath, navigate } = useAppRouter();
  const currentRoute = findRoute(currentPath);
  const [selectedProject, setSelectedProject] = useState<Project | null>(null);

  return (
    <AppShell
      currentPath={currentPath}
      onNavigate={navigate}
      selectedProject={selectedProject}
    >
      {currentPath === "/projects" ? (
        <ProjectsPage
          selectedProject={selectedProject}
          onSelectProject={setSelectedProject}
        />
      ) : (
        <WorkspacePage route={currentRoute} />
      )}
    </AppShell>
  );
}

export default App;
