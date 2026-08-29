import "./App.css";
import { AppShell } from "./layout/AppShell";
import { WorkspacePage } from "./pages/WorkspacePage";
import { findRoute } from "./routing/routes";
import { useAppRouter } from "./routing/useAppRouter";

function App() {
  const { currentPath, navigate } = useAppRouter();
  const currentRoute = findRoute(currentPath);

  return (
    <AppShell currentPath={currentPath} onNavigate={navigate}>
      <WorkspacePage route={currentRoute} />
    </AppShell>
  );
}

export default App;
