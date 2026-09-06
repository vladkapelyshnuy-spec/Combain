import { useState } from "react";
import "./App.css";
import { AppShell } from "./layout/AppShell";
import type { Project } from "./models/project";
import { MediaPage } from "./pages/MediaPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { ScriptScenePage } from "./pages/ScriptScenePage";
import { SettingsPage } from "./pages/SettingsPage";
import { StoryboardPage } from "./pages/StoryboardPage";
import { TimelinePage } from "./pages/TimelinePage";
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
      ) : currentPath === "/script" || currentPath === "/scenes" ? (
        <ScriptScenePage
          currentPath={currentPath}
          onNavigate={navigate}
          selectedProject={selectedProject}
        />
      ) : currentPath === "/storyboard" ? (
        <StoryboardPage
          onNavigate={navigate}
          selectedProject={selectedProject}
        />
      ) : currentPath === "/media" ? (
        <MediaPage selectedProject={selectedProject} onNavigate={navigate} />
      ) : currentPath === "/timeline" ? (
        <TimelinePage selectedProject={selectedProject} onNavigate={navigate} />
      ) : currentPath === "/settings" ? (
        <SettingsPage />
      ) : (
        <WorkspacePage route={currentRoute} />
      )}
    </AppShell>
  );
}

export default App;
