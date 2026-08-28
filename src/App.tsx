import "./App.css";
import { AppShell } from "./layout/AppShell";
import { HomePage } from "./pages/HomePage";
import { UiStatesPage } from "./pages/UiStatesPage";
import { useAppRouter } from "./routing/useAppRouter";

function App() {
  const { currentPath, navigate } = useAppRouter();

  return (
    <AppShell currentPath={currentPath} onNavigate={navigate}>
      {currentPath === "/ui-states" ? <UiStatesPage /> : <HomePage />}
    </AppShell>
  );
}

export default App;
